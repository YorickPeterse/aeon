//! Thread-safe reference counting pointers, without weak pointers.
//!
//! ArcWithoutWeak is a pointer similar to Rust's Arc type, except no weak
//! references are supported. This makes ArcWithoutWeak ideal for performance
//! sensitive code where weak references are not needed.
use std::cmp;
use std::mem;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The inner value of a pointer.
///
/// This uses the C representation to ensure that the value is always the first
/// member of this structure. This in turn allows one to read the value of this
/// `Inner` using `*mut T`.
#[repr(C)]
pub struct Inner<T> {
    value: T,
    references: AtomicUsize,
}

/// A thread-safe reference counted pointer.
pub struct ArcWithoutWeak<T> {
    inner: NonNull<Inner<T>>,
}

unsafe impl<T> Sync for ArcWithoutWeak<T> {}
unsafe impl<T> Send for ArcWithoutWeak<T> {}

impl<T> ArcWithoutWeak<T> {
    /// Consumes the `ArcWithoutWeak`, returning the wrapped pointer.
    ///
    /// The returned pointer is in reality a pointer to the inner structure,
    /// instead of a pointer directly to the value.
    pub fn into_raw(value: Self) -> *mut T {
        let raw = value.inner;

        mem::forget(value);

        raw.as_ptr() as _
    }

    /// Constructs an `ArcWithoutWeak` from a raw pointer.
    ///
    /// This method is incredibly unsafe, as it makes no attempt to verify if
    /// the pointer actually a pointer previously created using
    /// `ArcWithoutWeak::into_raw()`.
    pub unsafe fn from_raw(value: *mut T) -> Self {
        ArcWithoutWeak {
            inner: NonNull::new_unchecked(value as *mut Inner<T>),
        }
    }

    pub fn new(value: T) -> Self {
        let inner = Inner {
            value,
            references: AtomicUsize::new(1),
        };

        ArcWithoutWeak {
            inner: unsafe {
                NonNull::new_unchecked(Box::into_raw(Box::new(inner)))
            },
        }
    }

    pub fn inner(&self) -> &Inner<T> {
        unsafe { self.inner.as_ref() }
    }

    pub fn as_ptr(&self) -> *mut T {
        self.inner.as_ptr() as _
    }
}

impl<T> Deref for ArcWithoutWeak<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner().value
    }
}

impl<T> Clone for ArcWithoutWeak<T> {
    fn clone(&self) -> ArcWithoutWeak<T> {
        self.inner().references.fetch_add(1, Ordering::Relaxed);

        ArcWithoutWeak { inner: self.inner }
    }
}

impl<T> Drop for ArcWithoutWeak<T> {
    fn drop(&mut self) {
        unsafe {
            if self.inner().references.fetch_sub(1, Ordering::AcqRel) == 1 {
                let boxed = Box::from_raw(self.inner.as_mut());

                drop(boxed);
            }
        }
    }
}

impl<T: PartialOrd> PartialOrd for ArcWithoutWeak<T> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for ArcWithoutWeak<T> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: PartialEq> PartialEq for ArcWithoutWeak<T> {
    fn eq(&self, other: &Self) -> bool {
        (**self) == (**other)
    }
}

impl<T: Eq> Eq for ArcWithoutWeak<T> {}

impl<T> From<&ArcWithoutWeak<T>> for ArcWithoutWeak<T> {
    fn from(arc: &ArcWithoutWeak<T>) -> Self {
        arc.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn test_deref() {
        let pointer = ArcWithoutWeak::new(10);

        assert_eq!(*pointer, 10);
    }

    #[test]
    fn test_clone() {
        let pointer = ArcWithoutWeak::new(10);
        let cloned = pointer.clone();

        assert_eq!(pointer.inner().references.load(Ordering::SeqCst), 2);
        assert_eq!(cloned.inner().references.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_drop() {
        let pointer = ArcWithoutWeak::new(10);
        let cloned = pointer.clone();

        drop(cloned);

        assert_eq!(pointer.inner().references.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_cmp() {
        let foo = ArcWithoutWeak::new(10);
        let bar = ArcWithoutWeak::new(20);

        assert_eq!(foo.cmp(&bar), cmp::Ordering::Less);
        assert_eq!(foo.cmp(&foo), cmp::Ordering::Equal);
        assert_eq!(bar.cmp(&foo), cmp::Ordering::Greater);
    }

    #[test]
    fn test_partial_cmp() {
        let foo = ArcWithoutWeak::new(10);
        let bar = ArcWithoutWeak::new(20);

        assert_eq!(foo.partial_cmp(&bar), Some(cmp::Ordering::Less));
        assert_eq!(foo.partial_cmp(&foo), Some(cmp::Ordering::Equal));
        assert_eq!(bar.partial_cmp(&foo), Some(cmp::Ordering::Greater));
    }

    #[test]
    fn test_eq() {
        let foo = ArcWithoutWeak::new(10);
        let bar = ArcWithoutWeak::new(20);

        assert!(foo == foo);
        assert!(foo != bar);
    }

    #[test]
    fn test_type_size() {
        assert_eq!(mem::size_of::<ArcWithoutWeak<()>>(), 8);
        assert_eq!(mem::size_of::<Option<ArcWithoutWeak<()>>>(), 8);
    }
}
