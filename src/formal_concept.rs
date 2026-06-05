use crate::FormalContext;
use crate::bit_fiddling::*;

use bitvec::prelude::*;
use std::sync::Arc;

/// A formal concept tied to the context that produced it.
///
/// A concept is a closed pair of an extent and an intent. This type stores the
/// pair as [`RawFormalConcept`] data and keeps an [`Arc`] to the corresponding
/// [`FormalContext`], so methods can translate set bits back into object and
/// attribute labels.
#[derive(Debug, Clone)]
pub struct FormalConcept<A = String, B = String> {
    /// The context whose objects and attributes are indexed by `data`.
    ///
    /// This field is exported for ease of use. If you change the value of
    /// context after construction, the concept may become invalid in a number
    /// of ways.
    pub context: Arc<FormalContext<A, B>>,
    /// The raw extent and intent bitsets for this concept. This field is
    /// exported for ease of use, but changing it after construction may make
    /// the concept invalid.
    pub data: RawFormalConcept,
}

/// A formal concept represented only by object and attribute bitsets. The same
/// `RawFormalConcept` will mean different things (and be valid or invalid) in
/// different contexts. This type is used when performing computations with a
/// fixed context and the names of objects and attributes are irrelevant (e.g.
/// when enumerating concepts) to avoid overhead.
///
/// `extent` and `intent` are exported for ease of use, but changing them after
/// construction may make the concept invalid.
#[derive(Debug, Clone)]
pub struct RawFormalConcept {
    /// `extent[i]` is `true` if and only if the i-th object is in the concept extent.
    pub extent: BitVec,
    /// `intent[j]` is `true` if and only if the j-th attribute is in the concept intent.
    pub intent: BitVec,
}

impl RawFormalConcept {
    /// Converts this raw concept into a named [`FormalConcept`] for `context`.
    ///
    /// # Panics
    ///
    /// Panics if the bitset lengths do not match the context dimensions, or if
    /// the raw extent/intent pair is not closed in the supplied context.
    pub fn to_formal_concept<A, B>(self, context: Arc<FormalContext<A, B>>) -> FormalConcept<A, B> {
        assert_eq!(context.objects.len(), self.extent.len());
        assert_eq!(context.attributes.len(), self.intent.len());
        let result = FormalConcept {
            context,
            data: self,
        };
        assert!(result.validate());
        result
    }
}

impl<A: std::fmt::Debug, B: std::fmt::Debug> std::fmt::Display for FormalConcept<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let extent: Vec<_> = self.extent_names_iter().collect();
        let intent: Vec<_> = self.intent_names_iter().collect();
        write!(f, "Extent: {:?}, Intent: {:?}", extent, intent)
    }
}

impl<A, B> FormalConcept<A, B> {
    /// Returns `true` if and only if this concept is indeed a valid concept in its context.
    ///
    /// A valid concept must satisfy two conditions equations: inducing the intent
    /// from the extent gives the stored intent, and inducing the extent from the intent
    /// gives the stored extent. If either of these conditions fails, the concept is invalid.
    pub fn validate(&self) -> bool {
        self.data.extent == self.context.induce_l(&self.data.intent)
            && self.data.intent == self.context.induce_r(&self.data.extent)
    }

    /// Iterates over the object labels in this concept's extent.
    pub fn extent_names_iter(&self) -> impl Iterator<Item = &A> {
        self.data
            .extent
            .iter_ones()
            .map(|i| &self.context.objects[i])
    }

    /// Iterates over the attribute labels in this concept's intent.
    pub fn intent_names_iter(&self) -> impl Iterator<Item = &B> {
        self.data
            .intent
            .iter_ones()
            .map(|j| &self.context.attributes[j])
    }
}

impl<A: PartialEq, B: PartialEq> PartialEq for FormalConcept<A, B> {
    fn eq(&self, other: &Self) -> bool {
        *self.context == *other.context && self.data == other.data
    }
}

impl<A: Eq, B: Eq> Eq for FormalConcept<A, B> {}

impl<A: PartialEq, B: PartialEq> PartialOrd for FormalConcept<A, B> {
    /// Concepts are ordered by subset containment of their extents, provided they are from the same context.
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if *self.context != *other.context {
            return None; // Cannot compare concepts from different contexts
        }
        self.data.partial_cmp(&other.data)
    }
}

impl PartialEq for RawFormalConcept {
    fn eq(&self, other: &Self) -> bool {
        self.extent == other.extent
    }
}

impl Eq for RawFormalConcept {}

impl PartialOrd for RawFormalConcept {
    /// Concepts are ordered by subset containment of their extents.
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.extent == other.extent {
            return Some(std::cmp::Ordering::Equal);
        }
        if is_subset(&self.extent, &other.extent) {
            return Some(std::cmp::Ordering::Less);
        }
        if is_subset(&other.extent, &self.extent) {
            return Some(std::cmp::Ordering::Greater);
        }
        None
    }
}
