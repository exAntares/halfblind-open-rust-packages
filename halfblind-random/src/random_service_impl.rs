use crate::random_service::RandomService;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::sync::Mutex;

pub struct RandomServiceImpl {
    rng: Mutex<SmallRng>,
}

impl RandomService for RandomServiceImpl {
    fn random_range_usize(&self, min: usize, max: usize) -> usize {
        self.rng.lock().unwrap().random_range(min..=max)
    }

    fn random_range_i32(&self, min: i32, max: i32) -> i32 {
        self.rng.lock().unwrap().random_range(min..=max)
    }

    fn random_range_u32(&self, min: u32, max: u32) -> u32 {
        self.rng.lock().unwrap().random_range(min..=max)
    }

    fn random_range_f32(&self, min: f32, max: f32) -> f32 {
        self.rng.lock().unwrap().random_range(min..max)
    }

    fn random_range_u64(&self, min: u64, max: u64) -> u64 {
        self.rng.lock().unwrap().random_range(min..=max)
    }

    fn random_bool(&self) -> bool {
        self.rng.lock().unwrap().random_bool(0.5)
    }

    fn random_f64(&self) -> f64 {
        self.rng.lock().unwrap().random()
    }

    fn get_small_rng_clone(
        &self
    ) -> SmallRng {
        let mut guard = self.rng.lock().unwrap();
        let _:f64 = guard.random(); // Let the RNG advance a bit, otherwise it may happen that all frames return the same value
        guard.clone()
    }
}

impl RandomServiceImpl {
    pub fn new(
        seed: [u8; 32],
    ) -> Self {
        let rng = Mutex::new(SmallRng::from_seed(seed));
        Self {
            rng,
        }
    }
}