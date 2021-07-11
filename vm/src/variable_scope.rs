use crate::chunk::Chunk;
use crate::indexes::LocalIndex;
use crate::mem::allocator::Pointer;
use std::cell::UnsafeCell;
use std::rc::Rc;

/// Local variables that are available to a method and any closures defined
/// within.
///
/// Variable scopes use interior mutability for local variables, and are exposed
/// using a reference counting wrapper. The local variables are not modified
/// concurrently, as Inko doesn't allow concurrent access to the same binding.
///
/// We use reference counting as a VariableScope may be used by different
/// closures at the same time.
pub struct VariableScope {
    locals: UnsafeCell<Chunk<Pointer>>,
}

impl VariableScope {
    pub fn new(locals: u16) -> Rc<VariableScope> {
        Rc::new(VariableScope {
            locals: UnsafeCell::new(Chunk::new(locals as usize)),
        })
    }

    /// Returns the local variable for the given index/offset.
    pub fn get(&self, index: LocalIndex) -> Pointer {
        unsafe { *self.locals().get(index.into()) }
    }

    /// Sets the local variable for the given index/offset.
    pub fn set(&self, index: LocalIndex, value: Pointer) {
        unsafe {
            self.locals_mut().set(index.into(), value);
        }
    }

    fn locals(&self) -> &Chunk<Pointer> {
        unsafe { &*self.locals.get() }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    fn locals_mut(&self) -> &mut Chunk<Pointer> {
        unsafe { &mut *self.locals.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let binding = VariableScope::new(2);

        assert_eq!(binding.locals().len(), 2);
    }

    #[test]
    fn test_get_local_valid() {
        let ptr = unsafe { Pointer::new(0x1 as _) };
        let binding = VariableScope::new(1);
        let index = unsafe { LocalIndex::new(0) };

        binding.set(index, ptr);

        assert!(binding.get(index) == ptr);
    }

    #[test]
    fn test_set_local() {
        let ptr = unsafe { Pointer::new(0x1 as _) };
        let binding = VariableScope::new(1);
        let index = unsafe { LocalIndex::new(0) };

        binding.set(index, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_locals() {
        let ptr = unsafe { Pointer::new(0x1 as _) };
        let binding = VariableScope::new(1);

        binding.set(unsafe { LocalIndex::new(0) }, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_locals_mut() {
        let ptr = unsafe { Pointer::new(0x1 as _) };
        let binding = VariableScope::new(1);

        binding.set(unsafe { LocalIndex::new(0) }, ptr);

        assert_eq!(binding.locals_mut().len(), 1);
    }
}
