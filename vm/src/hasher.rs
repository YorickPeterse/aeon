//! Types and methods for hashing objects.
use ahash::AHasher;
use std::hash::{Hash, Hasher as HasherTrait};
use std::i64;
use std::u64;

#[derive(Clone)]
pub struct Hasher {
    hasher: AHasher,
    key1: u64,
    key2: u64,
}

impl Hasher {
    pub fn new(key1: u64, key2: u64) -> Self {
        Hasher {
            hasher: AHasher::new_with_keys(key1 as u128, key2 as u128),
            key1,
            key2,
        }
    }

    pub fn write_int(&mut self, value: i64) {
        value.hash(&mut self.hasher);
    }

    pub fn write_unsigned_int(&mut self, value: u64) {
        value.hash(&mut self.hasher);
    }

    pub fn write_float(&mut self, value: f64) {
        value.to_bits().hash(&mut self.hasher);
    }

    pub fn write_string(&mut self, value: &str) {
        value.hash(&mut self.hasher);
    }

    pub fn finish(&mut self) -> u64 {
        let hash = self.hasher.finish();

        self.hasher =
            AHasher::new_with_keys(self.key1 as u128, self.key2 as u128);

        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_write_int() {
        let mut hasher1 = Hasher::new(1, 2);
        let mut hasher2 = Hasher::new(1, 2);

        hasher1.write_int(10);
        hasher2.write_int(10);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn test_write_unsigned_int() {
        let mut hasher1 = Hasher::new(1, 2);
        let mut hasher2 = Hasher::new(1, 2);

        hasher1.write_unsigned_int(10);
        hasher2.write_unsigned_int(10);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn test_write_float() {
        let mut hasher1 = Hasher::new(1, 2);
        let mut hasher2 = Hasher::new(1, 2);

        hasher1.write_float(10.5);
        hasher2.write_float(10.5);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn test_write_string() {
        let mut hasher1 = Hasher::new(1, 2);
        let mut hasher2 = Hasher::new(1, 2);
        let string = "hello".to_string();

        hasher1.write_string(&string);
        hasher2.write_string(&string);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn test_to_hash_resets_hasher() {
        let mut hasher = Hasher::new(1, 2);

        hasher.write_float(10.5);
        let hash1 = hasher.finish();

        hasher.write_float(10.5);
        let hash2 = hasher.finish();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_mem_size() {
        assert!(size_of::<Hasher>() <= 64);
    }
}
