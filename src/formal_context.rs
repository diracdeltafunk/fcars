use std::fmt::Display;
use std::io::Read;

use crate::FormalConcept;
use crate::RawFormalConcept;
use crate::bit_fiddling::*;
use bitvec::prelude::*;
use std::sync::Arc;

/// A binary relation between objects and attributes.
///
/// One can query the relation by object and attribute indices with [`FormalContext::get_relation_idx`] or by labels with [`FormalContext::get_relation`]. The relation can be modified with [`FormalContext::modify_relation_idx`] or by labels with [`FormalContext::modify_relation`].
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct FormalContext<A = String, B = String> {
    /// Object labels.
    pub objects: Vec<A>,
    /// Attribute labels.
    pub attributes: Vec<B>,
    relation: Vec<BitVec>,            // The intent of each object
    relation_transposed: Vec<BitVec>, // The extent of each attribute
}

impl<A: Display, B: Display> Display for FormalContext<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Print header
        write!(f, "{:>10}", "")?;
        for attr in &self.attributes {
            write!(f, "{:>5}", attr)?;
        }
        writeln!(f)?;
        // Print each row
        for (i, obj) in self.objects.iter().enumerate() {
            write!(f, "{:>10}", obj)?;
            for j in 0..self.attributes.len() {
                let mark = if self.relation[i][j] { "1" } else { "0" };
                write!(f, "{:>5}", mark)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

impl<A, B> FormalContext<A, B> {
    /// Constructs a new formal context.
    ///
    /// The binary matrix is given by `relation`: `relation[i][j]` is true when
    /// `objects[i]` has `attributes[j]`. The constructor also builds and stores
    /// the transposed relation used by closure operations.
    ///
    /// # Panics
    ///
    /// Panics if `relation.len() != objects.len()` or if any row length differs
    /// from `attributes.len()`.
    pub fn new(objects: Vec<A>, attributes: Vec<B>, relation: Vec<BitVec>) -> Self {
        assert_eq!(relation.len(), objects.len());
        let mut relation_transposed = vec![BitVec::with_capacity(objects.len()); attributes.len()];
        for row in &relation {
            assert_eq!(row.len(), attributes.len());
            for (j, bit) in row.iter().by_vals().enumerate() {
                relation_transposed[j].push(bit);
            }
        }
        Self {
            objects,
            attributes,
            relation,
            relation_transposed,
        }
    }
    /// Checks that the formal context is well-formed.
    ///
    /// In memory, both the relation and its transpose are stored. This function
    /// makes sure they are consistent with each other.
    pub fn validate(&self) -> bool {
        // Check if relation is the transpose of relation_transposed
        for i in 0..self.objects.len() {
            for j in 0..self.attributes.len() {
                if self.relation[i][j] != self.relation_transposed[j][i] {
                    return false; // Relation and its transpose do not match
                }
            }
        }
        true
    }
    /// Creates a new formal context where no objects have any attributes.
    pub fn zero_context(objects: Vec<A>, attributes: Vec<B>) -> Self {
        Self {
            relation: vec![BitVec::repeat(false, attributes.len()); objects.len()],
            relation_transposed: vec![BitVec::repeat(false, objects.len()); attributes.len()],
            objects,
            attributes,
        }
    }
    /// Returns the maximal concept as a [`RawFormalConcept`].
    ///
    /// The maximal concept has all objects in its extent and the attributes
    /// common to every object in its intent.
    pub fn max_concept_raw(&self) -> RawFormalConcept {
        RawFormalConcept {
            extent: BitVec::repeat(true, self.objects.len()),
            intent: self
                .relation
                .iter()
                .fold(BitVec::repeat(true, self.attributes.len()), |a, b| a & b),
        }
    }
    /// Modifies the relation at the given object and attribute indices.
    ///
    /// Both the row-oriented relation and its transpose are updated.
    ///
    /// # Panics
    ///
    /// Panics if either index is out of bounds.
    pub fn modify_relation_idx(&mut self, obj_idx: usize, attr_idx: usize, value: bool) {
        self.relation[obj_idx].set(attr_idx, value);
        self.relation_transposed[attr_idx].set(obj_idx, value);
    }
    /// Returns the relation entry at the given object and attribute indices.
    ///
    /// # Panics
    ///
    /// Panics if either index is out of bounds.
    pub fn get_relation_idx(&self, obj_idx: usize, attr_idx: usize) -> bool {
        self.relation[obj_idx][attr_idx]
    }
    /// Returns the intent bitset for the object at index `i`.
    ///
    /// # Panics
    ///
    /// Panics if `i` is out of bounds.
    pub fn get_object_intent(&self, i: usize) -> &BitVec {
        &self.relation[i]
    }
    /// Returns the extent bitset for the attribute at index `i`.
    ///
    /// # Panics
    ///
    /// Panics if `i` is out of bounds.
    pub fn get_attribute_extent(&self, i: usize) -> &BitVec {
        &self.relation_transposed[i]
    }
    /// Induces the intent of an extent.
    ///
    /// `extent` is a bitset over object indices. The returned bitset contains
    /// the attributes common to all objects in the extent.
    pub fn induce_r(&self, extent: &BitVec) -> BitVec {
        let mut intent = BitVec::repeat(true, self.attributes.len());
        for obj in extent.iter_ones() {
            intent &= &self.relation[obj];
        }
        intent
    }
    /// Induces the extent of an intent.
    ///
    /// `intent` is a bitset over attribute indices. The returned bitset
    /// contains the objects that have every attribute in the intent.
    pub fn induce_l(&self, intent: &BitVec) -> BitVec {
        let mut extent = BitVec::repeat(true, self.objects.len());
        for attr in intent.iter_ones() {
            extent &= &self.relation_transposed[attr];
        }
        extent
    }
    /// Returns whether the context is reduced.
    ///
    /// A context is reduced when no row or column of the relation is the
    /// intersection of other rows or columns, respectively.
    pub fn is_reduced(&self) -> bool {
        redundant_row(&self.relation).is_none()
            && redundant_row(&self.relation_transposed).is_none()
    }
    /// Reduces this context in place.
    ///
    /// Redundant rows and columns are removed, so this can change `objects`,
    /// `attributes`, and the relation matrix.
    pub fn reduce(&mut self) {
        while let Some(i) = redundant_row(&self.relation) {
            self.objects.remove(i);
            self.relation.remove(i);
            for c in &mut self.relation_transposed {
                c.remove(i);
            }
        }
        while let Some(i) = redundant_row(&self.relation_transposed) {
            self.attributes.remove(i);
            self.relation_transposed.remove(i);
            for r in &mut self.relation {
                r.remove(i);
            }
        }
    }
    /// Returns the fraction of relation entries that are true.
    ///
    /// # Panics
    ///
    /// Panics if the context has no objects or no attributes.
    pub fn density(&self) -> f64 {
        if self.objects.is_empty() || self.attributes.is_empty() {
            panic!("Cannot compute density of empty context");
        }
        self.relation
            .iter()
            .map(|row| row.count_ones() as f64)
            .sum::<f64>()
            / (self.objects.len() * self.attributes.len()) as f64
    }
}

impl<A: Clone, B: Clone> FormalContext<A, B> {
    pub(crate) fn arc(&self) -> Arc<Self> {
        Arc::new(self.clone())
    }
    /// Returns the maximal concept as a [`FormalConcept`].
    /// See [`FormalContext::max_concept_raw`] for details.
    pub fn max_concept(&self) -> FormalConcept<A, B> {
        self.max_concept_raw()
            .to_formal_concept(std::sync::Arc::new(self.clone()))
    }
}

impl<A: Clone> FormalContext<A, A> {
    /// Creates the contranomial scale on the given labels.
    ///
    /// Each label is used both as an object and an attribute. Object `i` has
    /// every attribute except attribute `i`.
    pub fn contranomial_scale(objects: Vec<A>) -> Self {
        let mut relation = vec![BitVec::repeat(true, objects.len()); objects.len()];
        for (i, row) in relation.iter_mut().enumerate() {
            row.set(i, false);
        }
        Self {
            attributes: objects.clone(),
            relation_transposed: relation.clone(),
            relation,
            objects,
        }
    }
}

impl<A: Eq, B: Eq> FormalContext<A, B> {
    /// Returns whether object `obj` has attribute `attr`.
    ///
    /// <div class="warning">If there is more than one object or attribute with the same label, this will operate on the first match.</div>
    ///
    /// # Panics
    ///
    /// Panics if either label is not present in the context.
    pub fn get_relation(&self, obj: &A, attr: &B) -> bool {
        let Some(obj_idx) = self.objects.iter().position(|o| o == obj) else {
            panic!("Object not found in context");
        };
        let Some(attr_idx) = self.attributes.iter().position(|a| a == attr) else {
            panic!("Attribute not found in context");
        };
        self.relation[obj_idx][attr_idx]
    }
    /// Builds an extent bitset from object labels.
    ///
    /// <div class="warning">If there is more than one object with the same label, this will operate on the first match.</div>
    ///
    /// Labels that are not present in the context are ignored.
    pub fn extent_from_objects(&self, objs: impl IntoIterator<Item = A>) -> BitVec {
        let mut extent = BitVec::repeat(false, self.objects.len());
        for obj in objs {
            if let Some(idx) = self.objects.iter().position(|o| *o == obj) {
                extent.set(idx, true);
            }
        }
        extent
    }
    /// Builds an intent bitset from attribute labels.
    ///
    /// <div class="warning">If there is more than one attribute with the same label, this will operate on the first match.</div>
    ///
    /// Labels that are not present in the context are ignored.
    pub fn intent_from_attributes(&self, attrs: impl IntoIterator<Item = B>) -> BitVec {
        let mut intent = BitVec::repeat(false, self.attributes.len());
        for attr in attrs {
            if let Some(idx) = self.attributes.iter().position(|a| *a == attr) {
                intent.set(idx, true);
            }
        }
        intent
    }
    /// Modifies the relation entry identified by object and attribute labels.
    ///
    /// <div class="warning">If there is more than one object or attribute with the same label, this will operate on the first match.</div>
    ///
    /// # Panics
    ///
    /// Panics if either label is not present in the context.
    pub fn modify_relation(&mut self, obj: &A, attr: &B, value: bool) {
        let obj_idx = self
            .objects
            .iter()
            .position(|o| o == obj)
            .expect("Object not found in context");
        let attr_idx = self
            .attributes
            .iter()
            .position(|a| a == attr)
            .expect("Attribute not found in context");
        self.modify_relation_idx(obj_idx, attr_idx, value);
    }
}

impl FormalContext {
    /// Loads a formal context from Burmeister `.cxt` input.
    ///
    /// The expected format is:
    ///
    /// ```cxt
    /// B
    ///
    /// <num_objects>
    /// <num_attributes>
    ///
    /// <name of object 1>
    /// <name of object 2>
    /// ...
    /// <name of last object>
    /// <name of attribute 1>
    /// <name of attribute 2>
    /// ...
    /// <name of last attribute>
    /// <first row of context matrix>
    /// <second row of context matrix>
    /// ...
    /// <last row of context matrix>
    /// ```
    ///
    /// The blank lines and the first line containing `B` must be present. Each
    /// matrix row corresponds to one object and contains `.` for false and `X`
    /// for true.
    ///
    /// # Panics
    ///
    /// Panics if the input cannot be read as lines, is malformed, has invalid
    /// dimensions, or contains relation rows with invalid characters or lengths.
    pub fn from_cxt(input: impl Read) -> Self {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(input);
        let mut lines = reader.lines();

        // Skip the first line (should be "B")
        lines.next().expect("Missing first line").expect("IO error");

        // Skip the blank line
        lines.next().expect("Missing blank line").expect("IO error");

        // Read number of objects and attributes
        let num_objects: usize = lines
            .next()
            .expect("Missing number of objects")
            .expect("IO error")
            .trim()
            .parse()
            .expect("Invalid number of objects");

        let num_attributes: usize = lines
            .next()
            .expect("Missing number of attributes")
            .expect("IO error")
            .trim()
            .parse()
            .expect("Invalid number of attributes");

        // Skip the blank line
        lines.next().expect("Missing blank line").expect("IO error");

        // Read object names
        let mut objects = Vec::with_capacity(num_objects);
        for _ in 0..num_objects {
            let obj_name = lines
                .next()
                .expect("Missing object name")
                .expect("IO error")
                .trim()
                .to_string();
            objects.push(obj_name);
        }

        // Read attribute names
        let mut attributes = Vec::with_capacity(num_attributes);
        for _ in 0..num_attributes {
            let attr_name = lines
                .next()
                .expect("Missing attribute name")
                .expect("IO error")
                .trim()
                .to_string();
            attributes.push(attr_name);
        }

        // Read relation matrix
        let mut relation = Vec::with_capacity(num_objects);
        for _ in 0..num_objects {
            let row_str = lines
                .next()
                .expect("Missing relation row")
                .expect("IO error")
                .trim()
                .to_string();

            let mut row = BitVec::with_capacity(num_attributes);
            for ch in row_str.chars() {
                match ch {
                    'X' => row.push(true),
                    '.' => row.push(false),
                    _ => panic!("Invalid character in matrix!"),
                }
            }

            if row.len() != num_attributes {
                panic!("Row length doesn't match number of attributes");
            }

            relation.push(row);
        }
        Self::new(objects, attributes, relation)
    }
}

impl FormalContext<String, usize> {
    /// Loads a formal context from simple `.dat` input.
    ///
    /// Each line corresponds to one object and contains a space-separated list
    /// of non-negative integer attributes. Object labels are generated as
    /// `obj0`, `obj1`, and so on. Attribute labels are the parsed `usize`
    /// values, sorted increasingly.
    ///
    /// Empty lines are meaningful: they create objects with no attributes.
    ///
    /// # Panics
    ///
    /// Panics if the input cannot be read as lines or if any attribute token
    /// cannot be parsed as `usize`.
    pub fn from_dat(input: impl Read) -> Self {
        use std::collections::HashSet;
        use std::io::{BufRead, BufReader};

        let reader = BufReader::new(input);
        let lines = reader.lines();

        // Collect all unique attributes from all lines
        let mut all_attributes = HashSet::new();
        let mut object_attributes: Vec<Vec<usize>> = Vec::new();

        for line_result in lines {
            let attrs: Vec<usize> = line_result
                .expect("IO Error")
                .split_whitespace()
                .map(|s| s.parse().expect("Invalid attribute"))
                .collect();
            object_attributes.push(attrs.clone());
            for attr in attrs {
                all_attributes.insert(attr);
            }
        }

        let num_objects = object_attributes.len();

        let objects: Vec<String> = (0..num_objects).map(|i| format!("obj{}", i)).collect();

        let mut attributes: Vec<usize> = all_attributes.into_iter().collect();
        attributes.sort_unstable();

        let mut relation = vec![BitVec::repeat(false, attributes.len()); num_objects];

        for i in 0..num_objects {
            for att in &object_attributes[i] {
                let j = attributes
                    .binary_search(att)
                    .expect("Attribute not found in sorted attribute list");
                relation[i].set(j, true);
            }
        }

        Self::new(objects, attributes, relation)
    }
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_is_subset() {
        let a = bitvec![1, 0, 1];
        let b = bitvec![1, 1, 1];
        assert!(is_subset(&a, &b));
        assert!(!is_subset(&b, &a));
    }
    #[test]
    fn test_reduction() {
        let mut context = FormalContext {
            objects: vec!["a", "b", "c"],
            attributes: vec!["1", "2", "3"],
            relation: vec![
                bitvec![1, 0, 1], // a
                bitvec![1, 1, 1], // b
                bitvec![0, 1, 1], // c
            ],
            relation_transposed: vec![
                bitvec![1, 1, 0], // 1
                bitvec![0, 1, 1], // 2
                bitvec![1, 1, 1], // 3
            ],
        };
        assert!(!context.is_reduced());
        context.reduce();
        assert!(context.relation == vec![bitvec![1, 0], bitvec![0, 1]]);
        assert!(context.is_reduced());
    }

    #[test]
    fn test_from_dat_uses_usize_attributes() {
        let context = FormalContext::from_dat("2 10\n1 2\n10\n".as_bytes());

        assert_eq!(context.objects, vec!["obj0", "obj1", "obj2"]);
        assert_eq!(context.attributes, vec![1, 2, 10]);
        assert!(context.get_relation(&"obj0".to_string(), &2));
        assert!(context.get_relation(&"obj0".to_string(), &10));
        assert!(context.get_relation(&"obj1".to_string(), &1));
        assert!(!context.get_relation(&"obj1".to_string(), &10));
    }

    #[test]
    fn test_from_cxt_accepts_read_input() {
        let input = b"B

2
2

obj0
obj1
attr0
attr1
X.
.X
";

        let context = FormalContext::from_cxt(&input[..]);

        assert_eq!(context.objects, vec!["obj0", "obj1"]);
        assert_eq!(context.attributes, vec!["attr0", "attr1"]);
        assert!(context.get_relation(&"obj0".to_string(), &"attr0".to_string()));
        assert!(context.get_relation(&"obj1".to_string(), &"attr1".to_string()));
        assert!(!context.get_relation(&"obj0".to_string(), &"attr1".to_string()));
    }
}
