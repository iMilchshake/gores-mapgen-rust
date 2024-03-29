use crate::position::ShiftDirection;
use rand::prelude::*;
use rand::rngs::SmallRng;
use rand_distr::WeightedAliasIndex;
use seahash::hash;

pub struct Random {
    pub seed_str: Option<String>,
    pub seed_hex: String,
    pub seed_u64: u64,
    gen: SmallRng,
    weighted_dist: WeightedAliasIndex<i32>,
}

impl Random {
    pub fn new(seed_u64: u64, weights: Vec<i32>) -> Random {
        Random {
            seed_str: None,
            seed_u64,
            seed_hex: format!("{:X}", seed_u64),
            gen: SmallRng::seed_from_u64(seed_u64),
            weighted_dist: Random::get_weighted_dist(weights),
        }
    }

    pub fn from_str_seed(seed_str: String, weights: Vec<i32>) -> Random {
        let seed_u64 = hash(seed_str.as_bytes());
        let mut rnd = Random::new(seed_u64, weights);
        rnd.seed_str = Some(seed_str);

        rnd
    }

    pub fn from_previous_rnd(rnd: &mut Random, weights: Vec<i32>) -> Random {
        let seed_u64 = rnd.gen.next_u64();

        Random::new(seed_u64, weights)
    }

    pub fn random_u64(&mut self) -> u64 {
        self.gen.next_u64()
    }

    pub fn str_seed_to_u64(seed_str: &String) -> u64 {
        hash(seed_str.as_bytes())
    }

    fn get_weighted_dist(weights: Vec<i32>) -> WeightedAliasIndex<i32> {
        // sadly WeightedAliasIndex is initialized using a Vec. So im manually checking for the
        // correct size. I feel like there must be a better way also the current apprach allows
        // for invalid moves to be picked. But that should be no problem in pracise
        assert_eq!(weights.len(), 4);
        WeightedAliasIndex::new(weights).expect("expect valid weights")
    }

    /// sample a shift based on weight distribution
    pub fn sample_move(&mut self, shifts: [ShiftDirection; 4]) -> ShiftDirection {
        let index = self.weighted_dist.sample(&mut self.gen);
        *shifts.get(index).expect("out of bounds")
    }

    pub fn with_probability(&mut self, probability: f32) -> bool {
        self.gen.gen_bool(probability.into())
    }

    /// TODO: is this broken?
    pub fn pick_element<'a, T>(&'a mut self, values: &'a Vec<T>) -> &T {
        &values[self.gen.gen_range(0..values.len())]
    }

    pub fn random_circularity(&mut self) -> f32 {
        self.gen.gen_range(0.0..=1.0)
    }

    pub fn random_kernel_size(&mut self, max_size: usize) -> usize {
        assert!(max_size >= 1);
        self.gen.gen_range(1..=max_size)
    }
}
