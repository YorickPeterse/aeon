//! Functions for slicing operations on Vec and String types.
use crate::mem::allocator::Pointer;
use crate::mem::objects::Int;
use crate::numeric::Modulo;
use std::i128;
use std::i64;

// Returns the slice index to use for accessing an element, with support for
// negative indexes.
//
// When a negative index is used, the final index is determined by counting
// backwards. For example, for a slice with 3 values an index of -1 will map to
// an index of 2. Some other examples, all for a slice with 3 values:
//
// * -1 maps to 2
// * -2 maps to 1
// * -3 maps to 0
// * -4 maps to 2
pub fn slice_index_to_usize(index: Pointer, length: usize) -> usize {
    let val = unsafe { Int::read(index) };

    if val >= 0 {
        val as usize
    } else if length == 0 {
        0
    } else if (length as u64) > (i64::MAX as u64) {
        // If the number of values in a slice is greater than we can fit in
        // a i64 (= the type of the index), we upcast both to the index and
        // the length to a i128, then convert this to our final value.
        //
        // Casting the result back to usize should be safe because, as on a
        // 32 bits platform "length" is never greater than the maximum i64
        // value, and an index can never be greater than an i64 either.
        //
        // This is a bit of a hack, and I'm sure there's a better way of
        // doing this (apart from not supporting negative slice indexes).
        // Unfortunately, at the time of writing this was the best I could
        // come up with.
        (i128::from(val)).modulo(length as i128) as usize
    } else {
        val.modulo(length as i64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc_without_weak::ArcWithoutWeak;
    use crate::mem::allocator::{BlockAllocator, BumpAllocator};
    use crate::mem::objects::ClassPointer;

    #[test]
    fn test_slice_index_to_usize_with_no_values() {
        assert_eq!(slice_index_to_usize(Pointer::int(0), 0), 0);
        assert_eq!(slice_index_to_usize(Pointer::int(1), 0), 1);
        assert_eq!(slice_index_to_usize(Pointer::int(-5), 0), 0);
    }

    #[test]
    fn test_slice_index_to_usize_with_values() {
        let blocks = BlockAllocator::new();
        let mut alloc = BumpAllocator::new(ArcWithoutWeak::new(blocks));

        assert_eq!(slice_index_to_usize(Pointer::int(0), 3), 0);
        assert_eq!(slice_index_to_usize(Pointer::int(1), 3), 1);
        assert_eq!(slice_index_to_usize(Pointer::int(-1), 3), 2);
        assert_eq!(slice_index_to_usize(Pointer::int(-2), 3), 1);
        assert_eq!(slice_index_to_usize(Pointer::int(-3), 3), 0);
        assert_eq!(slice_index_to_usize(Pointer::int(-4), 3), 2);
        assert_eq!(slice_index_to_usize(Pointer::int(-5), 3), 1);
        assert_eq!(slice_index_to_usize(Pointer::int(-6), 3), 0);

        assert_eq!(
            slice_index_to_usize(Pointer::int(9_663_676_416), 10_737_418_240),
            9_663_676_416
        );

        assert_eq!(
            slice_index_to_usize(Pointer::int(-9_663_676_416), 10_737_418_240),
            1_073_741_824
        );

        assert_eq!(
            slice_index_to_usize(
                Int::alloc(
                    &mut alloc,
                    unsafe { ClassPointer::new(Pointer::new(0x4 as _)) },
                    -9_223_372_036_854_775_807
                ),
                18_446_744_073_709_551_615
            ),
            9_223_372_036_854_775_808
        );
    }
}
