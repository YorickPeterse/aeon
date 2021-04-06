//! Generic runtime Objects
//!
//! The Object struct is used to represent an object created during runtime. It
//! can be used to wrap native values (e.g. an integer or a string), look up
//! methods, add attributes, etc.
use ahash::AHashMap;
use std::ops::Drop;
use std::ptr;

use crate::object_pointer::{
    ObjectPointer, ObjectPointerPointer, RawObjectPointer,
};
use crate::object_value::ObjectValue;
use crate::tagged_pointer::TaggedPointer;

macro_rules! push_collection {
    ($map:expr, $what:ident, $vec:expr) => {{
        $vec.reserve($map.len());

        for thing in $map.$what() {
            $vec.push(*thing);
        }
    }};
}

/// The status of an object.
#[derive(Eq, PartialEq, Debug)]
pub enum ObjectStatus {
    /// This object is OK and no action has to be taken by a collector.
    OK,

    /// This object has been forwarded and all forwarding pointers must be
    /// resolved.
    Resolve,

    /// This object is ready to be promoted to the mature generation.
    Promote,

    /// This object should be evacuated from its block.
    Evacuate,

    /// This object is in the process of being moved.
    PendingMove,
}

pub type AttributesMap = AHashMap<ObjectPointer, ObjectPointer>;

/// The bit to set for objects that are being forwarded.
pub const PENDING_FORWARD_BIT: usize = 0;

/// The bit to set for objects that have been forwarded.
pub const FORWARDED_BIT: usize = 1;

/// The bit to set for objects stored in a remembered set.
pub const REMEMBERED_BIT: usize = 2;

/// The mask to apply when installing a forwarding pointer.
pub const FORWARDING_MASK: usize = 0x3;

/// Structure containing data of a single object.
pub struct Object {
    /// The prototype of this object.
    pub prototype: ObjectPointer,

    /// A pointer to the attributes of this object. Attributes are allocated
    /// on-demand and default to a NULL pointer.
    ///
    /// This pointer may be tagged to store extra information. The following
    /// lower bits can be set:
    ///
    /// * 000: this field contains a regular pointer.
    /// * 001: this object is in the process of being forwarded.
    /// * 010: this object has been forwarded, and this field is set to the
    ///   target `ObjectPointer`.
    /// * 100: this object has been remembered in the remembered set.
    ///
    /// Multiple bits can be set as well. For example, `101` would mean the
    /// object is remembered and being forwarded.
    pub attributes: TaggedPointer<AttributesMap>,

    /// A native Rust value (e.g. a String) that belongs to this object.
    pub value: ObjectValue,
}

unsafe impl Sync for Object {}
unsafe impl Send for Object {}

impl Object {
    /// Returns a new object with the given value.
    pub fn new(value: ObjectValue) -> Object {
        Object {
            prototype: ObjectPointer::null(),
            attributes: TaggedPointer::null(),
            value,
        }
    }

    /// Returns a new object with the given value and prototype.
    pub fn with_prototype(
        value: ObjectValue,
        prototype: ObjectPointer,
    ) -> Object {
        Object {
            prototype,
            attributes: TaggedPointer::null(),
            value,
        }
    }

    /// Sets the prototype of this object.
    pub fn set_prototype(&mut self, prototype: ObjectPointer) {
        self.prototype = prototype;
    }

    /// Returns the prototype of this object.
    pub fn prototype(&self) -> Option<ObjectPointer> {
        if self.prototype.is_null() {
            None
        } else {
            Some(self.prototype)
        }
    }

    /// Returns and removes the prototype of this object.
    pub fn take_prototype(&mut self) -> Option<ObjectPointer> {
        if self.prototype.is_null() {
            None
        } else {
            let proto = self.prototype;

            self.prototype = ObjectPointer::null();

            Some(proto)
        }
    }

    /// Removes an attribute and returns it.
    pub fn remove_attribute(
        &mut self,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map_mut() {
            map.remove(&name)
        } else {
            None
        }
    }

    /// Returns all the attributes available to this object.
    pub fn attributes(&self) -> Vec<ObjectPointer> {
        let mut attributes = Vec::new();

        if let Some(map) = self.attributes_map() {
            push_collection!(map, values, attributes);
        }

        attributes
    }

    /// Returns all the attribute names available to this object.
    pub fn attribute_names(&self) -> Vec<ObjectPointer> {
        let mut attributes = Vec::new();

        if let Some(map) = self.attributes_map() {
            push_collection!(map, keys, attributes);
        }

        attributes
    }

    /// Looks up an attribute in either the current object or a parent object.
    pub fn lookup_attribute(
        &self,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        let got = self.lookup_attribute_in_self(name);

        if got.is_some() {
            return got;
        }

        // Method defined somewhere in the object hierarchy
        if self.prototype().is_some() {
            let mut opt_parent = self.prototype();

            while let Some(parent_ptr) = opt_parent {
                let parent = parent_ptr.get();
                let got = parent.lookup_attribute_in_self(name);

                if got.is_some() {
                    return got;
                }

                opt_parent = parent.prototype();
            }
        }

        None
    }

    /// Adds a new attribute to the current object.
    pub fn add_attribute(
        &mut self,
        name: ObjectPointer,
        object: ObjectPointer,
    ) {
        self.allocate_attributes_map();

        self.attributes_map_mut().unwrap().insert(name, object);
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(
        &self,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if let Some(map) = self.attributes_map() {
            map.get(&name).cloned()
        } else {
            None
        }
    }

    /// Returns an immutable reference to the attributes.
    pub fn attributes_map(&self) -> Option<&AttributesMap> {
        self.attributes.as_ref()
    }

    pub fn attributes_map_mut(&self) -> Option<&mut AttributesMap> {
        self.attributes.as_mut()
    }

    pub fn set_attributes_map(&mut self, attrs: AttributesMap) {
        self.attributes = TaggedPointer::new(Box::into_raw(Box::new(attrs)));
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        if !self.prototype.is_null() {
            callback(self.prototype.pointer());
        }

        if let Some(map) = self.attributes_map() {
            // Attribute keys are interned strings, which don't need to be
            // marked.
            for (_, pointer) in map.iter() {
                callback(pointer.pointer());
            }
        }

        match self.value {
            ObjectValue::Array(ref array) => {
                for pointer in array.iter() {
                    callback(pointer.pointer());
                }
            }
            ObjectValue::Block(ref block) => {
                if let Some(captures_from) = block.captures_from.as_ref() {
                    captures_from.each_pointer(|v| callback(v));
                }

                callback(block.receiver.pointer());
            }
            ObjectValue::Binding(ref binding) => {
                binding.each_pointer(|v| callback(v));
            }
            ObjectValue::File(ref file) => {
                callback(file.path().pointer());
            }
            ObjectValue::Generator(ref gen) => {
                gen.each_pointer(|v| callback(v));
            }
            ObjectValue::Future(ref future) => {
                if let Some(ptr) = future.lock().result_pointer_pointer() {
                    callback(ptr);
                }
            }
            // Processes are not traced here, as otherwise we may end up tracing
            // processes other than the one that's being garbage collected.
            _ => {}
        }
    }

    /// Returns a new Object that takes over the data of the current object.
    pub fn take(&mut self) -> Object {
        let mut new_obj =
            Object::with_prototype(self.value.take(), self.prototype);

        // When taking over the attributes we want to automatically inherit the
        // "remembered" bit, but not the forwarding bits.
        let attrs = (self.attributes.raw as usize & !FORWARDING_MASK)
            as *mut AttributesMap;

        new_obj.attributes = TaggedPointer::new(attrs);

        // When the object is being forwarded we don't want to lose this status
        // by just setting the attributes to NULL. Doing so could result in
        // another collector thread to try and move the same object.
        self.attributes =
            TaggedPointer::with_bit(0x0 as _, PENDING_FORWARD_BIT);

        new_obj
    }

    /// Tries to mark this object as pending a forward.
    ///
    /// This method returns true if forwarding is necessary, false otherwise.
    pub fn mark_for_forward(&mut self) -> bool {
        // This _must_ be a reference, otherwise we'll be operating on a _copy_
        // of the pointer, since TaggedPointer is a Copy type.
        let current = &mut self.attributes;
        let current_raw = current.raw;

        if current.atomic_bit_is_set(PENDING_FORWARD_BIT) {
            // Another thread is in the process of forwarding this object, or
            // just finished forwarding it (since forward_to() sets both bits).
            return false;
        }

        let desired =
            TaggedPointer::with_bit(current_raw, PENDING_FORWARD_BIT).raw;

        current.compare_and_swap(current_raw, desired)
    }

    /// Forwards this object to the given pointer.
    pub fn forward_to(&mut self, pointer: ObjectPointer) {
        // We use a mask that sets the lower 2 bits, instead of only setting
        // one. This removes the need for checking both bits when determining if
        // forwarding is necessary.
        let new_attrs =
            (pointer.raw.raw as usize | FORWARDING_MASK) as *mut AttributesMap;

        self.attributes.atomic_store(new_attrs);
    }

    /// Marks this object as being remembered.
    ///
    /// This does not use atomic operations and thus should not be called
    /// concurrently for the same pointer.
    pub fn mark_as_remembered(&mut self) {
        self.attributes.set_bit(REMEMBERED_BIT);
    }

    /// Returns true if this object has been remembered in a remembered set.
    pub fn is_remembered(&self) -> bool {
        self.attributes.atomic_bit_is_set(REMEMBERED_BIT)
    }

    /// Returns true if this object is forwarded.
    pub fn is_forwarded(&self) -> bool {
        self.attributes.atomic_bit_is_set(FORWARDED_BIT)
    }

    /// Returns true if this object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        self.value.should_deallocate_native() || self.has_attributes()
    }

    /// Returns true if an attributes map has been allocated.
    pub fn has_attributes(&self) -> bool {
        if self.is_forwarded() {
            return false;
        }

        !self.attributes.untagged().is_null()
    }

    pub fn drop_attributes(&mut self) {
        if !self.has_attributes() {
            return;
        }

        drop(unsafe { Box::from_raw(self.attributes.untagged()) });

        self.attributes = TaggedPointer::null();
    }

    pub fn write_to(self, raw_pointer: RawObjectPointer) -> ObjectPointer {
        let pointer = ObjectPointer::new(raw_pointer);

        // Finalize the existing object, if needed. This must be done before we
        // allocate the new object, otherwise we will leak memory.
        pointer.finalize();

        // Write the new data to the pointer.
        unsafe {
            ptr::write(raw_pointer, self);
        }

        pointer
    }

    /// Allocates an attribute map if needed.
    fn allocate_attributes_map(&mut self) {
        if !self.has_attributes() {
            self.set_attributes_map(AttributesMap::default());
        }
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        self.drop_attributes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Binding;
    use crate::block::Block as CodeBlock;
    use crate::compiled_code::CompiledCode;
    use crate::config::Config;
    use crate::execution_context::ExecutionContext;
    use crate::file::File;
    use crate::generator::Generator;
    use crate::immix::block::Block;
    use crate::module::Module;
    use crate::object_pointer::{ObjectPointer, RawObjectPointer};
    use crate::object_value::ObjectValue;
    use crate::vm::state::State;
    use crate::vm::test::*;
    use std::mem;
    use std::path::PathBuf;

    fn fake_pointer() -> ObjectPointer {
        ObjectPointer::new(0x4 as RawObjectPointer)
    }

    fn new_object() -> Object {
        Object::new(ObjectValue::None)
    }

    fn object_pointer_for(object: &Object) -> ObjectPointer {
        ObjectPointer::new(object as *const Object as RawObjectPointer)
    }

    #[test]
    fn test_object_new() {
        let obj = new_object();

        assert!(obj.prototype.is_null());
        assert!(obj.attributes.is_null());
        assert!(obj.value.is_none());
    }

    #[test]
    fn test_object_with_prototype() {
        let obj = Object::with_prototype(ObjectValue::None, fake_pointer());

        assert_eq!(obj.prototype.is_null(), false);
        assert!(obj.attributes.is_null());
    }

    #[test]
    fn test_object_set_prototype() {
        let mut obj = new_object();

        assert!(obj.prototype.is_null());

        obj.set_prototype(fake_pointer());

        assert_eq!(obj.prototype.is_null(), false);
    }

    #[test]
    fn test_object_prototype() {
        let mut obj = new_object();

        assert!(obj.prototype().is_none());

        obj.set_prototype(fake_pointer());

        assert!(obj.prototype().is_some());
    }

    #[test]
    fn test_object_take_prototype() {
        let mut obj = new_object();

        obj.set_prototype(fake_pointer());

        assert!(obj.prototype().is_some());

        let proto = obj.take_prototype();

        assert!(proto.is_some());
        assert!(obj.prototype().is_none());
    }

    #[test]
    fn test_object_remove_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name, fake_pointer());

        let attr = obj.remove_attribute(name);

        assert!(attr.is_some());
        assert!(obj.lookup_attribute(name).is_none());
    }

    #[test]
    fn test_object_attributes() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());

        assert_eq!(obj.attributes().len(), 1);
    }

    #[test]
    fn test_object_attribute_names() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());

        assert_eq!(obj.attribute_names().len(), 1);
    }

    #[test]
    fn test_object_lookup_attribute_defined_in_receiver() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_defined_in_prototype() {
        let mut proto = new_object();
        let mut child = new_object();
        let name = fake_pointer();

        proto.add_attribute(name.clone(), fake_pointer());
        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_attribute(name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_with_prototype_without_attribute() {
        let proto = new_object();
        let mut child = new_object();
        let name = fake_pointer();

        child.set_prototype(object_pointer_for(&proto));

        assert!(child.lookup_attribute(name).is_none());
    }

    #[test]
    fn test_object_add_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(name).is_some());
    }

    #[test]
    fn test_object_lookup_attribute_without_attribute() {
        let obj = new_object();
        let name = fake_pointer();

        assert!(obj.lookup_attribute(name).is_none());
    }

    #[test]
    fn test_object_lookup_attribute_with_attribute() {
        let mut obj = new_object();
        let name = fake_pointer();

        obj.add_attribute(name.clone(), fake_pointer());

        assert!(obj.lookup_attribute(name).is_some());
    }

    #[test]
    fn test_object_attributes_map_without_map() {
        let obj = new_object();

        assert!(obj.attributes_map().is_none());
    }

    #[test]
    fn test_object_attributes_map_with_map() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());

        assert!(obj.attributes_map().is_some());
        assert!(obj.attributes_map_mut().is_some());
    }

    #[test]
    fn test_object_attributes_map_set_map() {
        let mut obj = new_object();
        let map = AttributesMap::default();

        obj.set_attributes_map(map);

        assert!(obj.attributes_map().is_some());
    }

    #[test]
    fn test_object_each_pointer_without_pointers() {
        let obj = new_object();
        let mut pointers = Vec::new();

        obj.each_pointer(|ptr| pointers.push(ptr));

        assert!(pointers.is_empty());
    }

    #[test]
    fn test_object_each_pointer_with_attributes() {
        let mut obj = new_object();
        let mut pointers = Vec::new();

        obj.add_attribute(fake_pointer(), fake_pointer());
        obj.each_pointer(|ptr| pointers.push(ptr));

        let pointer_pointer = pointers.pop().unwrap();

        pointer_pointer.get_mut().raw.raw = 0x5 as _;

        let value = obj.attributes()[0];

        assert_eq!(value.raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_each_pointer_with_array() {
        let mut pointers = Vec::new();
        let obj =
            Object::new(ObjectValue::Array(Box::new(vec![fake_pointer()])));

        obj.each_pointer(|ptr| pointers.push(ptr));

        let pointer_pointer = pointers.pop().unwrap();

        pointer_pointer.get_mut().raw.raw = 0x5 as _;

        let value = obj.value.as_array().unwrap()[0];

        assert_eq!(value.raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_each_pointer_with_block() {
        let state = State::with_rc(Config::new(), &[]);
        let binding = Binding::new(0, fake_pointer(), None);
        let name = state.intern_string("a".to_string());
        let path = state.intern_string("a.inko".to_string());
        let code = CompiledCode::new(name, path, 1, Vec::new());
        let module = Module::new(name, code, Vec::new());
        let block = CodeBlock::new(
            module.code(),
            Some(binding.clone()),
            fake_pointer(),
            &module,
        );

        let obj = Object::new(ObjectValue::Block(Box::new(block)));
        let mut pointers = Vec::new();

        obj.each_pointer(|ptr| pointers.push(ptr));

        while let Some(pointer_pointer) = pointers.pop() {
            pointer_pointer.get_mut().raw.raw = 0x5 as _;
        }

        assert_eq!(
            obj.value.as_block().unwrap().receiver.raw.raw as usize,
            0x5
        );

        assert_eq!(binding.receiver().raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_each_pointer_with_binding() {
        let binding = Binding::new(1, fake_pointer(), None);

        binding.set_local(0, fake_pointer());

        let obj = Object::new(ObjectValue::Binding(binding.clone()));
        let mut pointers = Vec::new();

        obj.each_pointer(|ptr| pointers.push(ptr));

        while let Some(pointer_pointer) = pointers.pop() {
            pointer_pointer.get_mut().raw.raw = 0x5 as _;
        }

        assert_eq!(binding.get_local(0).raw.raw as usize, 0x5);
        assert_eq!(binding.receiver().raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_each_pointer_with_prototype() {
        let mut obj = Object::new(ObjectValue::None);
        let mut pointers = Vec::new();

        obj.set_prototype(fake_pointer());
        obj.each_pointer(|ptr| pointers.push(ptr));

        while let Some(pointer_pointer) = pointers.pop() {
            pointer_pointer.get_mut().raw.raw = 0x5 as _;
        }

        assert_eq!(obj.prototype.raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_each_pointer_with_file() {
        let readme = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string();

        let state = State::with_rc(Config::new(), &[]);
        let path = state.intern_string(readme);
        let file = File::read_only(path).unwrap();
        let obj = Object::new(ObjectValue::File(Box::new(file)));
        let mut pointers = Vec::new();

        obj.each_pointer(|ptr| pointers.push(ptr));

        while let Some(pointer_pointer) = pointers.pop() {
            pointer_pointer.get_mut().raw.raw = 0x5 as _;
        }

        assert_eq!(obj.value.as_file().unwrap().path().raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_each_pointer_with_generator() {
        let (_machine, block, proc) = setup();
        let context = Box::new(ExecutionContext::from_block(&block));

        context.binding().set_local(0, fake_pointer());

        let gen = Generator::running(context);

        gen.yield_value(fake_pointer());

        let obj = proc.allocate_without_prototype(ObjectValue::Generator(gen));
        let mut pointers = Vec::new();

        obj.get().each_pointer(|ptr| pointers.push(ptr));

        while let Some(pointer_pointer) = pointers.pop() {
            pointer_pointer.get_mut().raw.raw = 0x5 as _;
        }

        let gen = obj.generator_value().unwrap();

        assert_eq!(gen.context().binding().get_local(0).raw.raw as usize, 0x5);
        assert_eq!(gen.take_result().unwrap().raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_take() {
        let mut obj = Object::new(ObjectValue::Float(10.0));
        let map = AttributesMap::default();

        obj.set_attributes_map(map);

        let new_obj = obj.take();

        assert!(obj.attributes_map().is_none());
        assert!(obj.value.is_none());

        assert!(new_obj.attributes_map().is_some());
        assert!(new_obj.value.is_float());
    }

    #[test]
    fn test_object_take_remembered_object() {
        let mut obj = Object::new(ObjectValue::Float(10.0));

        obj.mark_as_remembered();

        let new_obj = obj.take();

        assert_eq!(obj.is_remembered(), false);
        assert!(new_obj.is_remembered());
    }

    #[test]
    fn test_object_has_attributes() {
        let mut obj = Object::new(ObjectValue::Float(10.0));
        let map = AttributesMap::default();

        obj.set_attributes_map(map);

        assert!(obj.has_attributes());
    }

    #[test]
    fn test_object_has_attributes_forwarded() {
        let mut obj = Object::new(ObjectValue::Float(10.0));

        obj.attributes = TaggedPointer::new(0x1 as *mut _);
        obj.attributes.set_bit(PENDING_FORWARD_BIT);
        obj.attributes.set_bit(FORWARDED_BIT);

        assert_eq!(obj.has_attributes(), false);
    }

    #[test]
    fn test_object_has_attributes_remembered() {
        let mut obj = Object::new(ObjectValue::Float(10.0));
        let map = AttributesMap::default();

        obj.set_attributes_map(map);
        obj.mark_as_remembered();

        assert!(obj.has_attributes());
    }

    #[test]
    fn test_object_mark_as_remembered() {
        let mut obj = Object::new(ObjectValue::Float(10.0));

        assert_eq!(obj.is_remembered(), false);

        obj.mark_as_remembered();

        assert!(obj.is_remembered());
    }

    #[test]
    fn test_object_mark_as_forwarded_for_remembered_object() {
        let mut obj = Object::new(ObjectValue::Float(10.0));

        obj.mark_as_remembered();
        obj.mark_for_forward();

        assert!(obj.is_remembered());
        assert!(obj.attributes.bit_is_set(PENDING_FORWARD_BIT));
    }

    #[test]
    fn test_object_has_attributes_remembered_without_attributes() {
        let mut obj = Object::new(ObjectValue::Float(10.0));

        obj.mark_as_remembered();

        assert_eq!(obj.has_attributes(), false);
    }

    #[test]
    fn test_object_forward_to() {
        let mut obj = new_object();
        let target = new_object();

        obj.forward_to(object_pointer_for(&target));

        assert!(obj.is_forwarded());
        assert!(obj.attributes.bit_is_set(PENDING_FORWARD_BIT));
        assert!(obj.attributes.bit_is_set(FORWARDED_BIT));
    }

    #[test]
    fn test_object_forward_to_remembered_object() {
        let mut obj = new_object();
        let target = new_object();

        obj.mark_as_remembered();
        obj.forward_to(object_pointer_for(&target));

        assert!(obj.is_forwarded());
        assert!(obj.attributes.bit_is_set(PENDING_FORWARD_BIT));
        assert!(obj.attributes.bit_is_set(FORWARDED_BIT));

        assert_eq!(obj.is_remembered(), false);
    }

    #[test]
    fn test_object_size_of() {
        assert_eq!(mem::size_of::<Object>(), 32);
    }

    #[test]
    fn test_drop_attributes() {
        let mut obj = new_object();

        obj.add_attribute(fake_pointer(), fake_pointer());
        obj.drop_attributes();

        assert!(obj.attributes_map().is_none());
    }

    #[test]
    fn test_object_write_to() {
        let mut block = Block::boxed();
        let raw_pointer = block.request_pointer().unwrap();
        let pointer = ObjectPointer::new(raw_pointer);

        Object::new(ObjectValue::Float(10.5)).write_to(raw_pointer);

        assert_eq!(pointer.float_value().unwrap(), 10.5);
    }

    #[test]
    fn test_object_mark_for_forward() {
        let mut block = Block::boxed();
        let object = Object::new(ObjectValue::None);

        // We have to explicitly write the object to the pointer, otherwise our
        // pointer might point to random chunk, which could result in it
        // thinking the pending forward bit is already set.
        let pointer = object.write_to(block.request_pointer().unwrap());

        assert!(pointer.get_mut().mark_for_forward());
        assert!(pointer.get().attributes.bit_is_set(PENDING_FORWARD_BIT));

        assert_eq!(pointer.get_mut().mark_for_forward(), false);
    }

    #[test]
    fn test_object_mark_for_forward_with_previously_forwarded_pointer() {
        let mut block = Block::boxed();
        let raw_pointer = block.request_pointer().unwrap();

        {
            let pointer = Object::new(ObjectValue::None).write_to(raw_pointer);

            pointer.get_mut().mark_for_forward();
        }

        // This test ensures that `mark_for_forward` doesn't get messed up when
        // used on a slot that was previously forwarded (but first allocated
        // into again).
        let pointer = Object::new(ObjectValue::None).write_to(raw_pointer);

        assert_eq!(
            pointer.get().attributes.bit_is_set(PENDING_FORWARD_BIT),
            false
        );

        assert_eq!(pointer.get_mut().mark_for_forward(), true);
    }
}
