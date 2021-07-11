//! Traits and functions for various numerical operations.

/// Floored integer divisions.
///
/// The code for the implementations of this trait is taken from the num-integer
/// crate, which in turn used the algorithm from
/// http://research.microsoft.com/pubs/151917/divmodnote-letter.pdf.
///
/// Sources:
///
/// - https://github.com/rust-num/num-integer/blob/4d166cbb754244760e28ea4ce826d54fafd3e629/src/lib.rs#L412
/// - https://github.com/rust-num/num-integer/blob/4d166cbb754244760e28ea4ce826d54fafd3e629/src/lib.rs#L563
pub trait FlooredDiv<T = Self> {
    fn floored_division(self, rhs: T) -> T;
}

impl FlooredDiv for i64 {
    fn floored_division(self, rhs: i64) -> i64 {
        let (d, r) = (self / rhs, self % rhs);

        if (r > 0 && rhs < 0) || (r < 0 && rhs > 0) {
            d - 1
        } else {
            d
        }
    }
}

impl FlooredDiv for u64 {
    fn floored_division(self, rhs: u64) -> u64 {
        let (d, r) = (self / rhs, self % rhs);

        if r > 0 || rhs > 0 {
            d - 1
        } else {
            d
        }
    }
}

/// The Modulo trait is used for getting the modulo (instead of remainder) of a
/// number.
pub trait Modulo<T = Self> {
    fn modulo(self, rhs: T) -> T;
}

impl Modulo for i64 {
    fn modulo(self, rhs: i64) -> i64 {
        ((self % rhs) + rhs) % rhs
    }
}

impl Modulo for u64 {
    fn modulo(self, rhs: u64) -> u64 {
        ((self % rhs) + rhs) % rhs
    }
}

impl Modulo for i128 {
    fn modulo(self, rhs: i128) -> i128 {
        ((self % rhs) + rhs) % rhs
    }
}

/// Returns a u32 created from two separate u16 values.
pub fn u32_from_u16_pair(a: u16, b: u16) -> u32 {
    (u32::from(a) << 16) | (u32::from(b) & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulo_i64() {
        assert_eq!((-5_i64).modulo(86_400_i64), 86395_i64);
    }

    #[test]
    fn test_modulo_i128() {
        assert_eq!((-5_i128).modulo(86_400_i128), 86_395_i128);
    }

    #[test]
    fn test_u32_from_u16_pair() {
        assert_eq!(u32_from_u16_pair(18, 59_365), 1_239_013);
    }
}
