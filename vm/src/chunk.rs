//! Fixed-size chunks of memory.
use std::alloc::{self, Layout};
use std::mem;
use std::ops::Drop;
use std::ptr;
use std::slice;

/// A fixed-size amount of memory.
///
/// Chunks are a bit like a Vec, but with far fewer features and no bounds
/// checking. This makes them useful for cases where reads and writes are very
/// frequent, and an external source (e.g. a compiler) verifies if these
/// operations are within bounds.
///
/// A Chunk does not drop the values stored within, simply because we don't need
/// this at this time.
pub struct Chunk<T> {
    ptr: *mut T,
    capacity: usize,
}

unsafe fn layout_for<T>(capacity: usize) -> Layout {
    Layout::from_size_align_unchecked(
        mem::size_of::<T>() * capacity,
        mem::align_of::<T>(),
    )
}

#[cfg_attr(feature = "cargo-clippy", allow(clippy::len_without_is_empty))]
impl<T> Chunk<T> {
    pub fn new(capacity: usize) -> Self {
        if capacity == 0 {
            return Chunk {
                ptr: ptr::null_mut(),
                capacity: 0,
            };
        }

        let layout = unsafe { layout_for::<T>(capacity) };
        let ptr = unsafe { alloc::alloc(layout) as *mut T };

        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }

        Chunk { ptr, capacity }
    }

    pub fn from_ptr(ptr: *mut T, capacity: usize) -> Self {
        Chunk { ptr, capacity }
    }

    pub fn len(&self) -> usize {
        self.capacity
    }

    pub unsafe fn get(&self, index: usize) -> &T {
        &*self.ptr.add(index)
    }

    pub unsafe fn set(&mut self, index: usize, value: T) {
        self.ptr.add(index).write(value);
    }

    pub unsafe fn slice(&self, start: usize, length: usize) -> &[T] {
        slice::from_raw_parts(self.ptr.add(start), length)
    }

    pub unsafe fn slice_mut(
        &mut self,
        start: usize,
        length: usize,
    ) -> &mut [T] {
        slice::from_raw_parts_mut(self.ptr.add(start), length)
    }
}

impl<T> Drop for Chunk<T> {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                alloc::dealloc(
                    self.ptr as *mut u8,
                    layout_for::<T>(self.len()),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::allocator::Pointer;

    #[test]
    fn test_empty_chunk() {
        let chunk = Chunk::<()>::new(0);

        assert_eq!(chunk.len(), 0);
        assert!(chunk.ptr.is_null());
    }

    #[test]
    fn test_len() {
        let chunk = Chunk::<usize>::new(4);

        assert_eq!(chunk.len(), 4);
    }

    #[test]
    fn test_get_set() {
        let mut chunk = Chunk::<Pointer>::new(2);

        unsafe {
            chunk.set(0, Pointer::int(5));

            assert!(*chunk.get(0) == Pointer::int(5));
        }
    }

    #[test]
    fn test_slice() {
        let mut chunk = Chunk::<usize>::new(3);

        unsafe {
            chunk.set(0, 1);
            chunk.set(1, 2);
            chunk.set(2, 3);

            assert_eq!(chunk.slice(0, 3), &[1, 2, 3]);
            assert_eq!(chunk.slice(1, 2), &[2, 3]);
            assert_eq!(chunk.slice(0, 2), &[1, 2]);
        }
    }

    #[test]
    fn test_slice_mut() {
        let mut chunk = Chunk::<usize>::new(3);

        unsafe {
            chunk.set(0, 1);
            chunk.set(1, 2);
            chunk.set(2, 3);

            assert_eq!(chunk.slice_mut(0, 3), &[1, 2, 3]);
            assert_eq!(chunk.slice_mut(1, 2), &[2, 3]);
            assert_eq!(chunk.slice_mut(0, 2), &[1, 2]);
        }
    }
}
