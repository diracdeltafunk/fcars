use crate::FormalConcept;
use crate::FormalContext;
use crate::RawFormalConcept;
use crate::bit_fiddling::{DenseConcept, DenseContext, MaskConcept, MaskContext};
use rayon::iter::Either;
use rayon::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;

// PCbO state is carried as `(concept, y)`.
//
// `concept` is a closed pair `(extent, intent)`. `y` is the first
// attribute index this subtree is allowed to add. This is the usual CbO
// canonicity discipline: when we try to add attribute `j`, the generated
// closure is accepted only if it agrees with the parent intent on every
// attribute before `j`. That ensures every concept is generated once.
//
// There are two internal bitset engines, both defined in `bit_fiddling.rs`:
//
// 1. A fast scalar `u128` implementation. If both dimensions are <= u128::BITS, extents and
//    intents fit in registers, avoiding heap-backed bit vectors in the inner
//    PCbO loop. Public APIs still expose `RawFormalConcept`, so the fast path
//    converts back to `BitVec` at the boundary.
// 2. A dense word-slice implementation for larger contexts. It uses `Vec<u128>`
//    internally with a fixed bit ordering and just the operations FCA needs.
//
// `BitVec` remains the public representation, but it is no longer the inner
// PCbO workhorse. Both engines convert to `BitVec` only when a public API needs
// to yield or materialize a `RawFormalConcept`.

// The frontier factor controls how many top-level subtrees we create for Rayon:
// roughly `threads * factor`. Higher values improve load balancing when the
// search tree is uneven, but too many tiny subtrees increase scheduling and
// frontier-building overhead. The environment override is intentionally hidden
// but handy while profiling real datasets.
const PARALLEL_FRONTIER_FACTOR: usize = 8;

// One frame in the explicit DFS iterator used by the masked parallel iterator.
// `next_j` is the next candidate attribute to try for this frame.
// `yielded` records whether the frame's own concept has already been emitted.
struct MaskFrame {
    concept: MaskConcept,
    next_j: usize,
    yielded: bool,
}

// Iterator over one masked PCbO subtree. Rayon receives one of these per
// frontier subtree, so results can stream out without first allocating a giant
// `Vec<MaskConcept>` inside each worker.
struct MaskRawSubtreeIter {
    context: Arc<MaskContext>,
    stack: Vec<MaskFrame>,
}

// One frame in the explicit DFS iterator used by the dense parallel iterator.
// This is the multi-word analogue of `MaskFrame`.
struct DenseFrame {
    concept: DenseConcept,
    next_j: usize,
    yielded: bool,
}

// Iterator over one dense PCbO subtree. This has the same job as
// `MaskRawSubtreeIter`, but each concept stores multiple `u128` words.
struct DenseRawSubtreeIter {
    context: Arc<DenseContext>,
    stack: Vec<DenseFrame>,
}

impl<A: Sync, B: Sync> FormalContext<A, B> {
    // Build the `u128` fast-path context when the matrix fits in one mask per
    // extent/intent. Returning `None` sends larger contexts to the dense
    // multi-word implementation.
    fn mask_context(&self) -> Option<MaskContext> {
        MaskContext::from_bitvecs(
            self.objects.len(),
            self.attributes.len(),
            (0..self.objects.len()).map(|i| self.get_object_intent(i)),
            (0..self.attributes.len()).map(|i| self.get_attribute_extent(i)),
        )
    }

    // Build the arbitrary-size dense engine. This is used when the scalar
    // `u128` path cannot represent a whole extent/intent in one register.
    fn dense_context(&self) -> DenseContext {
        DenseContext::from_bitvecs(
            self.objects.len(),
            self.attributes.len(),
            (0..self.objects.len()).map(|i| self.get_object_intent(i)),
            (0..self.attributes.len()).map(|i| self.get_attribute_extent(i)),
        )
    }

    // Materialize all masked concepts into public `RawFormalConcept`s.
    //
    // This is fast for moderate workloads, but it is intentionally not the
    // best API for enormous lattices like S_5: materializing hundreds of
    // millions of heap-backed `BitVec` pairs is inherently expensive.
    fn all_concepts_raw_masked(&self, context: &MaskContext) -> Vec<RawFormalConcept> {
        let (prefix, frontier) = context.parallel_frontier();
        let mut concepts = prefix
            .into_iter()
            .map(|concept| context.to_raw_concept(concept))
            .collect::<Vec<_>>();
        let mut subtrees = frontier
            .into_par_iter()
            .flat_map_iter(|(concept, y)| {
                let mut concepts = Vec::new();
                context.collect_subtree(concept, y, &mut concepts);
                concepts
                    .into_iter()
                    .map(|concept| context.to_raw_concept(concept))
            })
            .collect();
        concepts.append(&mut subtrees);
        concepts
    }

    // Count concepts using the masked fast path, avoiding output conversion.
    fn num_concepts_masked(&self, context: &MaskContext) -> usize {
        let (prefix, frontier) = context.parallel_frontier();
        prefix.len()
            + frontier
                .into_par_iter()
                .map(|(concept, y)| context.count_subtree(concept, y))
                .sum::<usize>()
    }

    // Materialize all dense concepts into public `RawFormalConcept`s. Like the
    // masked materialization path, this is useful for moderate result sets but
    // unsuitable for lattices with hundreds of millions of concepts.
    fn all_concepts_raw_dense(&self, context: &DenseContext) -> Vec<RawFormalConcept> {
        let (prefix, frontier) = context.parallel_frontier();
        let mut concepts = prefix
            .into_iter()
            .map(|concept| context.to_raw_concept(&concept))
            .collect::<Vec<_>>();
        let mut subtrees = frontier
            .into_par_iter()
            .flat_map_iter(|(concept, y)| {
                let mut concepts = Vec::new();
                context.collect_subtree(concept, y, &mut concepts);
                concepts
                    .into_iter()
                    .map(|concept| context.to_raw_concept(&concept))
            })
            .collect();
        concepts.append(&mut subtrees);
        concepts
    }

    // Count concepts using the dense multi-word path, avoiding output
    // conversion. This is the large-context analogue of `num_concepts_masked`.
    fn num_concepts_dense(&self, context: &DenseContext) -> usize {
        let (prefix, frontier) = context.parallel_frontier();
        prefix.len()
            + frontier
                .into_par_iter()
                .map(|(concept, y)| context.count_subtree(concept, y))
                .sum::<usize>()
    }

    /// Returns a parallel iterator over all formal concepts for this formal context.
    pub fn all_concepts_raw_par_iter(&self) -> impl ParallelIterator<Item = RawFormalConcept> + '_ {
        if let Some(context) = self.mask_context() {
            // `impl ParallelIterator` requires one concrete return type. Rayon
            // `Either` lets the masked and generic branches share this public
            // signature while still using different internal iterator shapes.
            //
            // The masked branch stores the precomputed context in an `Arc`
            // because each Rayon worker owns an iterator over one subtree.
            let context = Arc::new(context);
            let (prefix, frontier) = context.parallel_frontier();
            let prefix_context = context.clone();
            return Either::Left(
                prefix
                    .into_par_iter()
                    .map(move |concept| prefix_context.to_raw_concept(concept))
                    .chain(frontier.into_par_iter().flat_map_iter(move |(concept, y)| {
                        MaskRawSubtreeIter::new(context.clone(), concept, y)
                    })),
            );
        }

        // Larger contexts use the same PCbO structure, but with explicit
        // `u128` word slices instead of general-purpose `BitVec` operations.
        let context = Arc::new(self.dense_context());
        let (prefix, frontier) = context.parallel_frontier();
        let prefix_context = context.clone();
        Either::Right(
            prefix
                .into_par_iter()
                .map(move |concept| prefix_context.to_raw_concept(&concept))
                .chain(frontier.into_par_iter().flat_map_iter(move |(concept, y)| {
                    DenseRawSubtreeIter::new(context.clone(), concept, y)
                })),
        )
    }

    pub fn all_concepts_raw(&self) -> Vec<RawFormalConcept> {
        // If the context fits the fast path, keep the inner traversal in masks
        // and convert once, at the API boundary.
        if let Some(context) = self.mask_context() {
            return self.all_concepts_raw_masked(&context);
        }

        self.all_concepts_raw_dense(&self.dense_context())
    }

    pub fn num_concepts(&self) -> usize {
        // Counting is the most efficient way to benchmark or size very large
        // lattices because it avoids allocating a `RawFormalConcept` per result.
        if let Some(context) = self.mask_context() {
            return self.num_concepts_masked(&context);
        }

        self.num_concepts_dense(&self.dense_context())
    }
}

impl MaskRawSubtreeIter {
    // Start a depth-first walk at one frontier root. The iterator yields the
    // root first, then explores children in increasing attribute order.
    fn new(context: Arc<MaskContext>, concept: MaskConcept, y: usize) -> Self {
        Self {
            context,
            stack: vec![MaskFrame {
                concept,
                next_j: y,
                yielded: false,
            }],
        }
    }
}

impl Iterator for MaskRawSubtreeIter {
    type Item = RawFormalConcept;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // `child` is held outside the borrow of `self.stack.last_mut()` so
            // we can push onto the stack after the mutable borrow of the top
            // frame ends.
            let mut child = None;

            {
                let frame = self.stack.last_mut()?;
                if !frame.yielded {
                    // Emit each frame's concept exactly once. The iterator is
                    // pre-order; order is not semantically important for the
                    // public parallel iterator, but pre-order keeps the state
                    // machine simple.
                    frame.yielded = true;
                    return Some(self.context.to_raw_concept(frame.concept));
                }

                // Resume scanning candidate attributes where this frame left
                // off. This is the explicit-stack equivalent of a recursive
                // `for j in y..attributes_len` loop.
                while frame.next_j < self.context.attributes_len() {
                    let j = frame.next_j;
                    frame.next_j += 1;

                    if self.context.concept_has_attribute(frame.concept, j) {
                        continue;
                    }

                    // `child` performs the extent intersection, closure, and
                    // canonicity check. `None` means the closure belongs to an
                    // earlier branch.
                    if let Some(concept) = self.context.child(frame.concept, j) {
                        child = Some((concept, j + 1));
                        break;
                    }
                }
            }

            if let Some((concept, y)) = child {
                // Descend to the next child. It inherits the canonical lower
                // bound `j + 1`, stored here as `y`.
                self.stack.push(MaskFrame {
                    concept,
                    next_j: y,
                    yielded: false,
                });
            } else {
                // No more children for this frame, so backtrack.
                self.stack.pop();
            }
        }
    }
}

impl MaskContext {
    // Generate scalar-mask children for frontier construction. Returning a
    // small `Vec` here is fine because this runs near the top of the tree, not
    // once per generated concept.
    fn children(&self, concept: MaskConcept, y: usize) -> Vec<(MaskConcept, usize)> {
        let mut result = Vec::new();
        for j in y..self.attributes_len() {
            if self.concept_has_attribute(concept, j) {
                continue;
            }

            if let Some(child) = self.child(concept, j) {
                result.push((child, j + 1));
            }
        }
        result
    }

    // Recursive masked collection for APIs that materialize a `Vec`. The
    // streaming parallel iterator uses `MaskRawSubtreeIter` instead, which
    // avoids temporary subtree vectors for large enumeration.
    fn collect_subtree(&self, concept: MaskConcept, y: usize, concepts: &mut Vec<MaskConcept>) {
        for j in y..self.attributes_len() {
            if self.concept_has_attribute(concept, j) {
                continue;
            }

            if let Some(child) = self.child(concept, j) {
                self.collect_subtree(child, j + 1, concepts);
            }
        }
        concepts.push(concept);
    }

    // Recursive masked count. This is the fastest path for huge lattices when
    // the caller only needs cardinality, because no public `BitVec`s are
    // allocated for individual concepts.
    fn count_subtree(&self, concept: MaskConcept, y: usize) -> usize {
        let mut count = 1;
        for j in y..self.attributes_len() {
            if self.concept_has_attribute(concept, j) {
                continue;
            }

            if let Some(child) = self.child(concept, j) {
                count += self.count_subtree(child, j + 1);
            }
        }
        count
    }

    // Build the same breadth-first Rayon frontier as the generic implementation
    // but with masked concepts. The prefix/frontier split avoids fine-grained
    // Rayon scheduling inside the deep PCbO recursion.
    fn parallel_frontier(&self) -> (Vec<MaskConcept>, Vec<(MaskConcept, usize)>) {
        let threads = rayon::current_num_threads();
        let target_frontier = if threads <= 1 {
            1
        } else {
            threads * PARALLEL_FRONTIER_FACTOR
        };
        let mut prefix = Vec::new();
        let mut frontier = VecDeque::from([(self.max_concept(), 0)]);

        while frontier.len() < target_frontier {
            let Some((concept, y)) = frontier.pop_front() else {
                break;
            };

            let children = self.children(concept, y);
            prefix.push(concept);
            frontier.extend(children);
        }

        (prefix, frontier.into())
    }
}

impl DenseRawSubtreeIter {
    // Start a streaming DFS over a dense frontier subtree.
    fn new(context: Arc<DenseContext>, concept: DenseConcept, y: usize) -> Self {
        Self {
            context,
            stack: vec![DenseFrame {
                concept,
                next_j: y,
                yielded: false,
            }],
        }
    }
}

impl Iterator for DenseRawSubtreeIter {
    type Item = RawFormalConcept;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Same state-machine shape as `MaskRawSubtreeIter`: inspect the
            // current frame, optionally discover one child, then push that child
            // after the mutable borrow of the frame has ended.
            let mut child = None;

            {
                let frame = self.stack.last_mut()?;
                if !frame.yielded {
                    // Yield before exploring descendants. Parallel iteration
                    // does not promise a stable global order, but each subtree
                    // still emits every concept exactly once.
                    frame.yielded = true;
                    return Some(self.context.to_raw_concept(&frame.concept));
                }

                // Continue scanning candidate attributes from where this frame
                // previously stopped.
                while frame.next_j < self.context.attributes_len() {
                    let j = frame.next_j;
                    frame.next_j += 1;

                    if self.context.concept_has_attribute(&frame.concept, j) {
                        continue;
                    }

                    // `child` performs both the extent intersection and
                    // canonicity check. `None` means the closure belongs to an
                    // earlier branch.
                    if let Some(concept) = self.context.child(&frame.concept, j) {
                        child = Some((concept, j + 1));
                        break;
                    }
                }
            }

            if let Some((concept, y)) = child {
                // Descend. `y` is always the candidate attribute plus one,
                // preserving the PCbO canonical generation order.
                self.stack.push(DenseFrame {
                    concept,
                    next_j: y,
                    yielded: false,
                });
            } else {
                // No candidates remain for the current frame.
                self.stack.pop();
            }
        }
    }
}

impl DenseContext {
    // Generate dense children for frontier construction. This is intentionally
    // vector-returning because it runs only near the top of the tree.
    fn children(&self, concept: &DenseConcept, y: usize) -> Vec<(DenseConcept, usize)> {
        let mut result = Vec::new();
        for j in y..self.attributes_len() {
            if self.concept_has_attribute(concept, j) {
                continue;
            }

            if let Some(child) = self.child(concept, j) {
                result.push((child, j + 1));
            }
        }
        result
    }

    // Recursive materialization helper for `all_concepts_raw`. The public
    // parallel iterator uses `DenseRawSubtreeIter` instead so it can stream.
    fn collect_subtree(&self, concept: DenseConcept, y: usize, concepts: &mut Vec<DenseConcept>) {
        for j in y..self.attributes_len() {
            if self.concept_has_attribute(&concept, j) {
                continue;
            }

            if let Some(child) = self.child(&concept, j) {
                self.collect_subtree(child, j + 1, concepts);
            }
        }
        concepts.push(concept);
    }

    // Recursive count helper. This avoids converting dense concepts back to
    // `BitVec`, so it is the preferred path for measuring huge lattices.
    fn count_subtree(&self, concept: DenseConcept, y: usize) -> usize {
        let mut count = 1;
        for j in y..self.attributes_len() {
            if self.concept_has_attribute(&concept, j) {
                continue;
            }

            if let Some(child) = self.child(&concept, j) {
                count += self.count_subtree(child, j + 1);
            }
        }
        count
    }

    // Build enough top-level dense subtrees for Rayon to balance work.
    fn parallel_frontier(&self) -> (Vec<DenseConcept>, Vec<(DenseConcept, usize)>) {
        let threads = rayon::current_num_threads();
        let target_frontier = if threads <= 1 {
            1
        } else {
            threads * PARALLEL_FRONTIER_FACTOR
        };
        let mut prefix = Vec::new();
        let mut frontier = VecDeque::from([(self.max_concept(), 0)]);

        while frontier.len() < target_frontier {
            let Some((concept, y)) = frontier.pop_front() else {
                break;
            };

            let children = self.children(&concept, y);
            prefix.push(concept);
            frontier.extend(children);
        }

        (prefix, frontier.into())
    }
}

impl<A: Clone + Send + Sync, B: Clone + Send + Sync> FormalContext<A, B> {
    pub fn all_concepts_par_iter(&self) -> impl ParallelIterator<Item = FormalConcept<A, B>> {
        let arc = self.arc();
        self.all_concepts_raw_par_iter()
            .map(move |data| FormalConcept {
                context: arc.clone(),
                data,
            })
    }
    pub fn all_concepts(&self) -> Vec<FormalConcept<A, B>> {
        self.all_concepts_par_iter().collect()
    }
}
