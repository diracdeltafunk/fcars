use bitvec::prelude::*;

use crate::RawFormalConcept;

pub(crate) fn is_subset(a: &BitVec, b: &BitVec) -> bool {
    if a.len() != b.len() {
        return false; // Different lengths, cannot be subset
    }
    let mut temp = a.clone();
    temp &= b;
    temp == *a
}

/// Determines if any row of the binary matrix x is an intersection of other rows
/// If so, returns the index of the first such row
/// Else, returns None
/// ASSUMES x is a matrix, i.e. each bitvec in x has the same length.
pub(crate) fn redundant_row(x: &[BitVec]) -> Option<usize> {
    for i in 0..x.len() {
        let mut best_approx = BitVec::repeat(true, x[0].len());
        for j in 0..x.len() {
            if i != j && is_subset(&x[i], &x[j]) {
                best_approx &= &x[j];
            }
        }
        if best_approx == x[i] {
            // Row i is the intersection of other rows
            return Some(i);
        }
    }
    None
}

// A concept in the scalar fast path. Bit `i` means object/attribute `i` is
// present. The scalar path is used when both dimensions fit in one `u128`.
#[derive(Clone, Copy)]
pub(crate) struct MaskConcept {
    pub(crate) extent: u128,
    pub(crate) intent: u128,
}

// Precomputed scalar form of a context. `object_intents[i]` is row `i`, and
// `attribute_extents[j]` is column `j`.
pub(crate) struct MaskContext {
    objects_len: usize,
    attributes_len: usize,
    object_intents: Vec<u128>,
    attribute_extents: Vec<u128>,
}

// A concept in the arbitrary-size dense path. These buffers may contain
// multiple `u128` words; dead bits in the final word are always zeroed.
#[derive(Clone)]
pub(crate) struct DenseConcept {
    pub(crate) extent: Vec<u128>,
    pub(crate) intent: Vec<u128>,
}

// Dense, flat storage for larger contexts. Rows are concatenated:
// `object_intents[obj * attr_words ..][..attr_words]` is one object intent.
// `attribute_extents[attr * obj_words ..][..obj_words]` is one attribute extent.
pub(crate) struct DenseContext {
    objects_len: usize,
    attributes_len: usize,
    obj_words: usize,
    attr_words: usize,
    object_intents: Vec<u128>,
    attribute_extents: Vec<u128>,
    object_tail_mask: u128,
    attribute_tail_mask: u128,
}

impl MaskContext {
    pub(crate) fn from_bitvecs<'a>(
        objects_len: usize,
        attributes_len: usize,
        object_intents: impl IntoIterator<Item = &'a BitVec>,
        attribute_extents: impl IntoIterator<Item = &'a BitVec>,
    ) -> Option<Self> {
        if objects_len > u128::BITS as usize || attributes_len > u128::BITS as usize {
            return None;
        }

        Some(Self {
            objects_len,
            attributes_len,
            object_intents: object_intents.into_iter().map(bitvec_to_mask).collect(),
            attribute_extents: attribute_extents.into_iter().map(bitvec_to_mask).collect(),
        })
    }

    pub(crate) fn attributes_len(&self) -> usize {
        self.attributes_len
    }

    pub(crate) fn concept_has_attribute(&self, concept: MaskConcept, attribute: usize) -> bool {
        bit_is_set(concept.intent, attribute)
    }

    // The maximal concept starts with all objects. Its intent is the
    // intersection of every object intent, i.e. the attributes common to all
    // objects.
    pub(crate) fn max_concept(&self) -> MaskConcept {
        let extent = low_bits(self.objects_len);
        let intent = self
            .object_intents
            .iter()
            .copied()
            .fold(low_bits(self.attributes_len), |a, b| a & b);
        MaskConcept { extent, intent }
    }

    // Try to create the child obtained by adding `attribute` to `concept`.
    // Returns `None` when the closure fails the PCbO canonicity test.
    pub(crate) fn child(&self, concept: MaskConcept, attribute: usize) -> Option<MaskConcept> {
        let extent = concept.extent & self.attribute_extents[attribute];
        let intent = self.induce_r(extent);

        if ((concept.intent ^ intent) & low_bits(attribute)) == 0 {
            Some(MaskConcept { extent, intent })
        } else {
            None
        }
    }

    // Derive the intent common to all objects in `extent`.
    //
    // `objects &= objects - 1` clears the lowest set bit, so the loop visits
    // exactly the objects in the extent without scanning every object index.
    fn induce_r(&self, extent: u128) -> u128 {
        let mut intent = low_bits(self.attributes_len);
        let mut objects = extent;
        while objects != 0 {
            let object = objects.trailing_zeros() as usize;
            intent &= self.object_intents[object];
            objects &= objects - 1;
        }
        intent
    }

    // Convert a masked concept back to the public representation. This is kept
    // at the boundary so the hot closure/canonicity work can stay allocation
    // free for small contexts.
    pub(crate) fn to_raw_concept(&self, concept: MaskConcept) -> RawFormalConcept {
        RawFormalConcept {
            extent: mask_to_bitvec(concept.extent, self.objects_len),
            intent: mask_to_bitvec(concept.intent, self.attributes_len),
        }
    }
}

impl DenseContext {
    pub(crate) fn from_bitvecs<'a>(
        objects_len: usize,
        attributes_len: usize,
        object_intents: impl IntoIterator<Item = &'a BitVec>,
        attribute_extents: impl IntoIterator<Item = &'a BitVec>,
    ) -> Self {
        let obj_words = words_for_bits(objects_len);
        let attr_words = words_for_bits(attributes_len);
        let object_tail_mask = tail_mask(objects_len);
        let attribute_tail_mask = tail_mask(attributes_len);

        let mut dense_object_intents = Vec::with_capacity(objects_len * attr_words);
        for intent in object_intents {
            dense_object_intents.extend(bitvec_to_dense_words(
                intent,
                attr_words,
                attribute_tail_mask,
            ));
        }

        let mut dense_attribute_extents = Vec::with_capacity(attributes_len * obj_words);
        for extent in attribute_extents {
            dense_attribute_extents.extend(bitvec_to_dense_words(
                extent,
                obj_words,
                object_tail_mask,
            ));
        }

        Self {
            objects_len,
            attributes_len,
            obj_words,
            attr_words,
            object_intents: dense_object_intents,
            attribute_extents: dense_attribute_extents,
            object_tail_mask,
            attribute_tail_mask,
        }
    }

    pub(crate) fn attributes_len(&self) -> usize {
        self.attributes_len
    }

    pub(crate) fn concept_has_attribute(&self, concept: &DenseConcept, attribute: usize) -> bool {
        dense_bit_is_set(&concept.intent, attribute)
    }

    // Maximal concept: all objects, and the attributes common to all objects.
    pub(crate) fn max_concept(&self) -> DenseConcept {
        let extent = full_dense_words(self.obj_words, self.object_tail_mask);
        let mut intent = full_dense_words(self.attr_words, self.attribute_tail_mask);
        for object in 0..self.objects_len {
            let object_intent = self.object_intent(object);
            for (intent_word, object_word) in intent.iter_mut().zip(object_intent) {
                *intent_word &= *object_word;
            }
        }

        DenseConcept { extent, intent }
    }

    // Try to create the child obtained by adding `attribute` to `concept`.
    // Returns `None` when the closure fails the PCbO canonicity test.
    pub(crate) fn child(&self, concept: &DenseConcept, attribute: usize) -> Option<DenseConcept> {
        let attribute_extent = self.attribute_extent(attribute);
        let mut extent = Vec::with_capacity(self.obj_words);
        for (left, right) in concept.extent.iter().zip(attribute_extent) {
            extent.push(left & right);
        }

        let mut intent = vec![0; self.attr_words];
        self.induce_r_into(&extent, &mut intent);

        if dense_prefix_eq(&concept.intent, &intent, attribute) {
            Some(DenseConcept { extent, intent })
        } else {
            None
        }
    }

    // Convert a dense concept back into the public `RawFormalConcept` type.
    pub(crate) fn to_raw_concept(&self, concept: &DenseConcept) -> RawFormalConcept {
        RawFormalConcept {
            extent: dense_words_to_bitvec(&concept.extent, self.objects_len),
            intent: dense_words_to_bitvec(&concept.intent, self.attributes_len),
        }
    }

    // Return the dense row for one object.
    fn object_intent(&self, object: usize) -> &[u128] {
        let start = object * self.attr_words;
        &self.object_intents[start..start + self.attr_words]
    }

    // Return the dense column for one attribute.
    fn attribute_extent(&self, attribute: usize) -> &[u128] {
        let start = attribute * self.obj_words;
        &self.attribute_extents[start..start + self.obj_words]
    }

    // Derive the intent common to all objects in `extent`, writing into a
    // caller-provided buffer.
    fn induce_r_into(&self, extent: &[u128], intent: &mut [u128]) {
        intent.fill(u128::MAX);
        if let Some(last) = intent.last_mut() {
            *last &= self.attribute_tail_mask;
        }

        for (word_idx, &word) in extent.iter().enumerate() {
            let mut objects = word;
            while objects != 0 {
                let object = word_idx * u128::BITS as usize + objects.trailing_zeros() as usize;
                let object_intent = self.object_intent(object);
                for attr_word in 0..self.attr_words {
                    intent[attr_word] &= object_intent[attr_word];
                }
                objects &= objects - 1;
            }
        }
    }
}

fn bitvec_to_mask(bits: &BitVec) -> u128 {
    bits.iter_ones().fold(0, |mask, bit| mask | (1_u128 << bit))
}

// Convert a `u128` mask to the public `BitVec` representation.
pub(crate) fn mask_to_bitvec(mask: u128, len: usize) -> BitVec {
    let words = len.div_ceil(usize::BITS as usize);
    let mut storage = Vec::with_capacity(words);
    for word in 0..words {
        storage.push((mask >> (word * usize::BITS as usize)) as usize);
    }

    let mut bits = BitVec::from_vec(storage);
    bits.truncate(len);
    bits
}

fn bitvec_to_dense_words(bits: &BitVec, words: usize, tail_mask: u128) -> Vec<u128> {
    let mut out = vec![0; words];
    for bit in bits.iter_ones() {
        out[bit / u128::BITS as usize] |= 1_u128 << (bit % u128::BITS as usize);
    }
    if let Some(last) = out.last_mut() {
        *last &= tail_mask;
    }
    out
}

fn dense_words_to_bitvec(words: &[u128], len: usize) -> BitVec {
    let storage_words = len.div_ceil(usize::BITS as usize);
    let mut storage = Vec::with_capacity(storage_words);

    for word in 0..storage_words {
        let bit = word * usize::BITS as usize;
        let dense_word = bit / u128::BITS as usize;
        let dense_shift = bit % u128::BITS as usize;
        storage.push((words[dense_word] >> dense_shift) as usize);
    }

    let mut bits = BitVec::from_vec(storage);
    bits.truncate(len);
    bits
}

pub(crate) fn bit_is_set(mask: u128, bit: usize) -> bool {
    (mask & (1_u128 << bit)) != 0
}

fn dense_bit_is_set(words: &[u128], bit: usize) -> bool {
    bit_is_set(words[bit / u128::BITS as usize], bit % u128::BITS as usize)
}

fn dense_prefix_eq(a: &[u128], b: &[u128], bits: usize) -> bool {
    let whole_words = bits / u128::BITS as usize;
    let rem_bits = bits % u128::BITS as usize;

    if a[..whole_words] != b[..whole_words] {
        return false;
    }

    rem_bits == 0 || ((a[whole_words] ^ b[whole_words]) & low_bits(rem_bits)) == 0
}

fn words_for_bits(bits: usize) -> usize {
    bits.div_ceil(u128::BITS as usize)
}

fn tail_mask(bits: usize) -> u128 {
    let rem = bits % u128::BITS as usize;
    if rem == 0 { u128::MAX } else { low_bits(rem) }
}

fn full_dense_words(words: usize, tail_mask: u128) -> Vec<u128> {
    let mut out = vec![u128::MAX; words];
    if let Some(last) = out.last_mut() {
        *last &= tail_mask;
    }
    out
}

pub(crate) fn low_bits(bits: usize) -> u128 {
    if bits == u128::BITS as usize {
        u128::MAX
    } else {
        (1_u128 << bits) - 1
    }
}
