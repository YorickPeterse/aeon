use crate::chunk::Chunk;
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use std::cell::UnsafeCell;
use std::rc::Rc;

/// A collection of local variables, a receiver, and an optional parent binding.
///
/// Every scope has a binding, and can be captured by one or more closures.
///
/// Bindings use interior mutability for local variables, and are exposed using
/// a reference counting wrapper. The local variables are not modified
/// concurrently, and the data is managed by the garbage collector; meaning
/// writes won't invalidate them. Thus, using this type should be sound in
/// practise.
pub struct Binding {
    /// The local variables in the current binding.
    locals: UnsafeCell<Chunk<ObjectPointer>>,

    /// The receiver (= the type of `self`) of the binding.
    receiver: ObjectPointer,

    /// The parent binding, if any.
    parent: Option<RcBinding>,
}

pub type RcBinding = Rc<Binding>;

impl Binding {
    pub fn new(
        locals: u16,
        receiver: ObjectPointer,
        parent: Option<RcBinding>,
    ) -> RcBinding {
        Rc::new(Binding {
            locals: UnsafeCell::new(Chunk::new(locals as usize)),
            receiver,
            parent,
        })
    }

    /// Returns the value of a local variable.
    pub fn get_local(&self, index: u16) -> ObjectPointer {
        self.locals()[index as usize]
    }

    /// Sets a local variable.
    pub fn set_local(&self, index: u16, value: ObjectPointer) {
        self.locals_mut()[index as usize] = value;
    }

    /// Returns true if the local variable exists.
    pub fn local_exists(&self, index: u16) -> bool {
        !self.get_local(index).is_null()
    }

    pub fn reset_locals(&self) {
        self.locals_mut().reset();
    }

    pub fn receiver(&self) -> &ObjectPointer {
        &self.receiver
    }

    /// Returns the parent binding.
    pub fn parent(&self) -> Option<&RcBinding> {
        self.parent.as_ref()
    }

    /// Returns an immutable reference to the parent binding, `depth` steps up
    /// from the current binding.
    pub fn find_parent(&self, depth: usize) -> Option<&RcBinding> {
        let mut found = self.parent.as_ref();

        for _ in 0..depth {
            if let Some(binding) = found {
                found = binding.parent.as_ref();
            } else {
                return None;
            }
        }

        found
    }

    /// Returns an immutable reference to this binding's local variables.
    pub fn locals(&self) -> &Chunk<ObjectPointer> {
        unsafe { &*self.locals.get() }
    }

    /// Returns a mutable reference to this binding's local variables.
    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn locals_mut(&self) -> &mut Chunk<ObjectPointer> {
        unsafe { &mut *self.locals.get() }
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        let mut current = Some(self);

        while let Some(binding) = current {
            callback(binding.receiver().pointer());

            for index in 0..binding.locals().len() {
                let local = &binding.locals()[index];

                if !local.is_null() {
                    callback(local.pointer());
                }
            }

            current = binding.parent.as_deref();
        }
    }

    /// Creates a new binding and recursively copies over all pointers to the
    /// target heap.
    pub fn clone_to<H: CopyObject>(
        &self,
        heap: &mut H,
    ) -> Result<RcBinding, String> {
        let parent = if let Some(ref bind) = self.parent {
            Some(bind.clone_to(heap)?)
        } else {
            None
        };

        let locals = self.locals();
        let mut new_locals = Chunk::new(locals.len());

        for index in 0..locals.len() {
            let pointer = locals[index];

            if !pointer.is_null() {
                new_locals[index] = heap.copy_object(pointer)?;
            }
        }

        let receiver_copy = heap.copy_object(*self.receiver())?;

        let new_binding = Rc::new(Binding {
            locals: UnsafeCell::new(new_locals),
            receiver: receiver_copy,
            parent,
        });

        Ok(new_binding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::immix::global_allocator::GlobalAllocator;
    use crate::immix::local_allocator::LocalAllocator;
    use crate::object_pointer::ObjectPointer;
    use crate::object_value;

    fn binding_with_parent(parent: RcBinding, locals: u16) -> RcBinding {
        Binding::new(locals, ObjectPointer::integer(1), Some(parent.clone()))
    }

    #[test]
    fn test_new() {
        let binding = Binding::new(2, ObjectPointer::integer(1), None);

        assert_eq!(binding.locals().len(), 2);
    }

    #[test]
    fn test_with_parent() {
        let binding1 = Binding::new(0, ObjectPointer::integer(1), None);
        let binding2 = binding_with_parent(binding1.clone(), 1);

        assert!(binding2.parent.is_some());
        assert_eq!(binding2.locals().len(), 1);
    }

    #[test]
    fn test_get_local_valid() {
        let ptr = ObjectPointer::integer(5);
        let binding = Binding::new(1, ObjectPointer::integer(1), None);

        binding.set_local(0, ptr);

        assert!(binding.get_local(0) == ptr);
    }

    #[test]
    fn test_set_local() {
        let ptr = ObjectPointer::integer(5);
        let binding = Binding::new(1, ObjectPointer::integer(1), None);

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_local_exists_non_existing_local() {
        let binding = Binding::new(1, ObjectPointer::integer(1), None);

        assert_eq!(binding.local_exists(0), false);
    }

    #[test]
    fn test_local_exists_existing_local() {
        let ptr = ObjectPointer::integer(5);
        let binding = Binding::new(1, ObjectPointer::integer(1), None);

        binding.set_local(0, ptr);

        assert!(binding.local_exists(0));
    }

    #[test]
    fn test_parent_without_parent() {
        let binding = Binding::new(0, ObjectPointer::integer(1), None);

        assert!(binding.parent().is_none());
    }

    #[test]
    fn test_parent_with_parent() {
        let binding1 = Binding::new(0, ObjectPointer::integer(1), None);
        let binding2 = binding_with_parent(binding1, 0);

        assert!(binding2.parent().is_some());
    }

    #[test]
    fn test_find_parent_without_parent() {
        let binding = Binding::new(0, ObjectPointer::integer(1), None);

        assert!(binding.find_parent(0).is_none());
    }

    #[test]
    fn test_find_parent_with_parent() {
        let binding1 = Binding::new(0, ObjectPointer::integer(1), None);
        let binding2 = binding_with_parent(binding1, 0);
        let binding3 = binding_with_parent(binding2, 0);
        let binding4 = binding_with_parent(binding3, 0);

        assert!(binding4.find_parent(0).is_some());
        assert!(binding4.find_parent(0).unwrap().parent().is_some());

        assert!(binding4.find_parent(1).is_some());
        assert!(binding4.find_parent(1).unwrap().parent().is_some());

        assert!(binding4.find_parent(2).is_some());
        assert!(binding4.find_parent(2).unwrap().parent().is_none());

        assert!(binding4.find_parent(3).is_none());
    }

    #[test]
    fn test_locals() {
        let ptr = ObjectPointer::integer(5);
        let binding = Binding::new(1, ObjectPointer::integer(1), None);

        binding.set_local(0, ptr);

        assert_eq!(binding.locals().len(), 1);
    }

    #[test]
    fn test_locals_mut() {
        let ptr = ObjectPointer::integer(5);
        let binding = Binding::new(1, ObjectPointer::integer(1), None);

        binding.set_local(0, ptr);

        assert_eq!(binding.locals_mut().len(), 1);
    }

    #[test]
    fn test_each_pointer() {
        let mut alloc =
            LocalAllocator::new(GlobalAllocator::with_rc(), &Config::new());

        let local1 = alloc.allocate_empty();
        let receiver = alloc.allocate_empty();
        let binding1 = Binding::new(1, receiver, None);

        binding1.set_local(0, local1);

        let local2 = alloc.allocate_empty();
        let binding2 = Binding::new(1, receiver, Some(binding1.clone()));

        binding2.set_local(0, local2);

        let mut pointer_pointers = Vec::new();

        binding2.each_pointer(|ptr| pointer_pointers.push(ptr));

        let pointers: Vec<_> =
            pointer_pointers.into_iter().map(|x| *x.get()).collect();

        assert_eq!(pointers.iter().filter(|x| **x == receiver).count(), 2);
        assert!(pointers.contains(&local2));
        assert!(pointers.contains(&local1));
    }

    #[test]
    fn test_each_pointer_and_update() {
        let mut alloc =
            LocalAllocator::new(GlobalAllocator::with_rc(), &Config::new());

        let binding = Binding::new(1, alloc.allocate_empty(), None);
        let mut pointers = Vec::new();

        binding.set_local(0, alloc.allocate_empty());

        binding.each_pointer(|ptr| pointers.push(ptr));

        while let Some(pointer_pointer) = pointers.pop() {
            let pointer = pointer_pointer.get_mut();

            pointer.raw.raw = 0x4 as _;
        }

        assert_eq!(binding.get_local(0).raw.raw as usize, 0x4);
        assert_eq!(binding.receiver().raw.raw as usize, 0x4);
    }

    #[test]
    fn test_clone_to() {
        let global_alloc = GlobalAllocator::with_rc();
        let mut alloc1 =
            LocalAllocator::new(global_alloc.clone(), &Config::new());
        let mut alloc2 = LocalAllocator::new(global_alloc, &Config::new());

        let ptr1 = alloc1.allocate_without_prototype(object_value::float(5.0));
        let ptr2 = alloc1.allocate_without_prototype(object_value::float(2.0));
        let ptr3 = alloc1.allocate_without_prototype(object_value::float(8.0));

        let src_bind1 = Binding::new(1, ptr3, None);
        let src_bind2 = Binding::new(1, ptr3, Some(src_bind1.clone()));

        src_bind1.set_local(0, ptr1);
        src_bind2.set_local(0, ptr2);

        let bind_copy = src_bind2.clone_to(&mut alloc2).unwrap();

        assert_eq!(bind_copy.locals().len(), 1);
        assert!(bind_copy.parent.is_some());

        assert_eq!(bind_copy.get_local(0).float_value().unwrap(), 2.0);

        let bind_copy_parent = bind_copy.parent.as_ref().unwrap();

        assert_eq!(bind_copy_parent.locals().len(), 1);
        assert!(bind_copy_parent.parent.is_none());

        assert_eq!(bind_copy_parent.get_local(0).float_value().unwrap(), 5.0);
        assert_eq!(bind_copy_parent.receiver().float_value().unwrap(), 8.0);
    }

    #[test]
    fn test_receiver_with_receiver() {
        let pointer = ObjectPointer::integer(5);
        let binding = Binding::new(1, pointer, None);

        assert!(*binding.receiver() == pointer);
    }
}
