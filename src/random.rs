use crate::FormalContext;
use bitvec::prelude::*;
use rand::Rng;

impl FormalContext<usize, usize> {
    /// Generate a random formal context with the given number of objects and attributes.
    /// Each entry in the matrix is 0 or 1 with probability `density`
    pub fn random_with_density(num_objs: usize, num_attrs: usize, density: f64) -> Self {
        let mut rng = rand::rng();
        let data: Vec<BitVec> = (0..num_objs)
            .map(|_| {
                (0..num_attrs)
                    .map(|_| rng.random_bool(density))
                    .collect::<BitVec>()
            })
            .collect();
        FormalContext::new((0..num_objs).collect(), (0..num_attrs).collect(), data)
    }
    /// Generate a random formal context with the given number of objects and attributes.
    /// Each entry in the matrix is 0 or 1 with probability 50%
    pub fn random(num_objs: usize, num_attrs: usize) -> Self {
        random_with_density(num_objs, num_attrs, 0.5)
    }
}
