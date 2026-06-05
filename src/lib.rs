#![warn(missing_docs)]

//! Formal Concept Analysis data structures and algorithms.
//!
//! The usual workflow is:
//!
//! 1. Build or load a [`FormalContext`].
//! 2. Use [`FormalContext::num_concepts`] when only the number of concepts is needed.
//! 3. Use [`FormalContext::all_concepts`] to enumerate [`FormalConcept`]s.
//!
//! Contexts can be constructed directly with [`FormalContext::new`], loaded from
//! Burmeister `.cxt` input with [`FormalContext::from_cxt`], or loaded from
//! simple space-separated `.dat` input with [`FormalContext::from_dat`].
//!
//! [`FormalConcept`] is the ergonomic concept type: it keeps an `Arc` pointer to
//! its context and can iterate over object and attribute labels.
//!
//! [`RawFormalConcept`]
//! is the lower-level representation; it stores only bitmasks of object and
//! attribute indices.
//!
//! The optional `random` feature adds constructors for random contexts:
//! you can specify the number of objects and attributes, and the desired
//! (expected) density of the context.

mod bit_fiddling;
mod formal_concept;
mod formal_context;
mod pcbo;
#[cfg(feature = "random")]
mod random;

pub use formal_concept::*;
pub use formal_context::*;

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use bitvec::prelude::*;
    #[test]
    fn test_pcbo_1() {
        let context = FormalContext::new(
            vec!["a", "b", "c"],
            vec!["1", "2", "3"],
            vec![
                bitvec![1, 0, 1], // a
                bitvec![1, 1, 1], // b
                bitvec![0, 1, 1], // c
            ],
        );
        assert_eq!(context.num_concepts(), 4);
    }
    #[test]
    fn test_pcbo_2() {
        // "Lives in Water"
        let context = FormalContext::new(
            vec![
                "fish leech",
                "bream",
                "frog",
                "dog",
                "water weeds",
                "reed",
                "bean",
                "corn",
            ],
            vec![
                "needs water to live",
                "lives in water",
                "lives on land",
                "needs chlorophyll",
                "dicotyledon",
                "monocotyledon",
                "can move",
                "has limbs",
                "breast feeds",
            ],
            vec![
                bitvec![1, 1, 0, 0, 0, 0, 1, 0, 0], // fish leech
                bitvec![1, 1, 0, 0, 0, 0, 1, 1, 0], // bream
                bitvec![1, 1, 1, 0, 0, 0, 1, 1, 0], // frog
                bitvec![1, 0, 1, 0, 0, 0, 1, 1, 1], // dog
                bitvec![1, 1, 0, 1, 0, 1, 0, 0, 0], // water weeds
                bitvec![1, 1, 1, 1, 0, 1, 0, 0, 0], // reed
                bitvec![1, 0, 1, 1, 1, 0, 0, 0, 0], // bean
                bitvec![1, 0, 1, 1, 0, 1, 0, 0, 0], // corn
            ],
        );
        assert_eq!(context.num_concepts(), 19);
        let concepts = context.all_concepts();
        assert_eq!(concepts.len(), 19);
        assert!(concepts.iter().all(FormalConcept::validate));
    }

    #[test]
    fn test_pcbo_dense_path() {
        let context = FormalContext::zero_context((0..129).collect(), (0..129).collect());
        assert_eq!(context.num_concepts(), 2);
        let concepts = context.all_concepts();
        assert_eq!(concepts.len(), 2);
        assert!(concepts.iter().all(FormalConcept::validate));
    }
}
