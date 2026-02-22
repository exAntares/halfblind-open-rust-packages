use rand::rngs::SmallRng;

pub trait RandomService: Send + Sync {
    fn random_range_usize(&self, min: usize, max: usize) -> usize;
    fn random_range_i32(&self, min: i32, max: i32) -> i32;
    fn random_range_u32(&self, min: u32, max: u32) -> u32;
    fn random_range_f32(&self, min: f32, max: f32) -> f32;
    fn random_range_u64(&self, min: u64, max: u64) -> u64;
    fn random_bool(&self) -> bool;
    fn random_f64(&self) -> f64;
    fn get_small_rng_clone(&self) -> SmallRng;
}
