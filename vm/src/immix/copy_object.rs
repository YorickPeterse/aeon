//! Copying Objects
//!
//! The CopyObject trait can be implemented by allocators to support copying of
//! objects into a heap.
use crate::block::Block;
use crate::object::{AttributesMap, Object};
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::object_value::ObjectValue;

pub trait CopyObject: Sized {
    /// Allocates a copied object.
    fn allocate_copy(&mut self, _: Object) -> ObjectPointer;

    /// Performs a deep copy of the given pointer.
    ///
    /// The copy of the input object is allocated on the current heap.
    fn copy_object(
        &mut self,
        to_copy_ptr: ObjectPointer,
    ) -> Result<ObjectPointer, String> {
        if to_copy_ptr.is_permanent() {
            return Ok(to_copy_ptr);
        }

        let to_copy = to_copy_ptr.get();

        // Copy over the object value
        let value_copy = match to_copy.value {
            ObjectValue::None => object_value::none(),
            ObjectValue::Float(num) => object_value::float(num),
            ObjectValue::Integer(num) => object_value::integer(num),
            ObjectValue::BigInt(ref bigint) => {
                ObjectValue::BigInt(bigint.clone())
            }
            ObjectValue::String(ref string) => {
                ObjectValue::String(string.clone())
            }
            ObjectValue::InternedString(ref string) => {
                ObjectValue::InternedString(string.clone())
            }
            ObjectValue::Array(ref raw_vec) => {
                let mut new_map = Vec::with_capacity(raw_vec.len());

                for value in raw_vec.iter() {
                    new_map.push(self.copy_object(*value)?);
                }

                object_value::array(new_map)
            }
            ObjectValue::File(ref file) => {
                object_value::file(file.clone_to(self)?)
            }
            ObjectValue::Block(ref block) => {
                let captures_from =
                    if let Some(bind) = block.captures_from.as_ref() {
                        Some(bind.clone_to(self)?)
                    } else {
                        None
                    };

                let receiver = self.copy_object(block.receiver)?;
                let new_block = Block::new(
                    block.code,
                    captures_from,
                    receiver,
                    &block.module,
                );

                object_value::block(new_block)
            }
            ObjectValue::Binding(ref binding) => {
                object_value::binding(binding.clone_to(self)?)
            }
            ObjectValue::Hasher(ref hasher) => {
                ObjectValue::Hasher((*hasher).clone())
            }
            ObjectValue::ByteArray(ref byte_array) => {
                ObjectValue::ByteArray(byte_array.clone())
            }
            ObjectValue::Library(ref val) => ObjectValue::Library(val.clone()),
            ObjectValue::Function(ref val) => {
                ObjectValue::Function(val.clone())
            }
            ObjectValue::Pointer(val) => ObjectValue::Pointer(val),
            ObjectValue::Process(ref proc) => {
                ObjectValue::Process(proc.clone())
            }
            ObjectValue::Socket(ref socket) => {
                ObjectValue::Socket(socket.clone())
            }
            ObjectValue::Module(ref module) => {
                ObjectValue::Module(module.clone())
            }
            ObjectValue::Generator(_) => {
                return Err("Generator objects can't be copied".to_string());
            }
            ObjectValue::Future(_) => {
                return Err("Future objects can't be copied".to_string());
            }
            ObjectValue::ExternalFunction(fun) => {
                ObjectValue::ExternalFunction(fun)
            }
            ObjectValue::Command(_) => {
                return Err("Commands can't be copied".to_string());
            }
        };

        let mut copy = if let Some(proto_ptr) = to_copy.prototype() {
            let proto_copy = self.copy_object(proto_ptr)?;

            Object::with_prototype(value_copy, proto_copy)
        } else {
            Object::new(value_copy)
        };

        if let Some(map) = to_copy.attributes_map() {
            let mut map_copy = AttributesMap::default();

            for (key, val) in map.iter() {
                let key_copy = self.copy_object(*key)?;
                let val_copy = self.copy_object(*val)?;

                map_copy.insert(key_copy, val_copy);
            }

            copy.set_attributes_map(map_copy);
        }

        Ok(self.allocate_copy(copy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Binding;
    use crate::compiled_code::CompiledCode;
    use crate::config::Config;
    use crate::immix::global_allocator::GlobalAllocator;
    use crate::immix::local_allocator::LocalAllocator;
    use crate::module::Module;
    use crate::object::Object;
    use crate::object_pointer::ObjectPointer;
    use crate::object_value;
    use crate::vm::state::{RcState, State};

    struct DummyAllocator {
        pub allocator: LocalAllocator,
    }

    impl DummyAllocator {
        pub fn new() -> DummyAllocator {
            let global_alloc = GlobalAllocator::with_rc();

            DummyAllocator {
                allocator: LocalAllocator::new(global_alloc, &Config::new()),
            }
        }
    }

    impl CopyObject for DummyAllocator {
        fn allocate_copy(&mut self, object: Object) -> ObjectPointer {
            self.allocator.allocate_copy(object)
        }
    }

    fn state() -> RcState {
        State::with_rc(Config::new(), &[])
    }

    #[test]
    fn test_copy_none() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy.allocator.allocate_empty();
        let copy = dummy.copy_object(pointer).unwrap();

        assert!(copy.get().value.is_none());
    }

    #[test]
    fn test_copy_with_prototype() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy.allocator.allocate_empty();
        let proto = dummy.allocator.allocate_empty();

        pointer.get_mut().set_prototype(proto);

        let copy = dummy.copy_object(pointer).unwrap();

        assert!(copy.get().prototype().is_some());
    }

    #[test]
    fn test_copy_with_attributes() {
        let mut dummy = DummyAllocator::new();
        let ptr1 = dummy.allocator.allocate_empty();
        let ptr2 = dummy.allocator.allocate_empty();
        let name = dummy.allocator.allocate_empty();

        ptr1.get_mut().add_attribute(name, ptr2);

        let copy = dummy.copy_object(ptr1).unwrap();

        assert!(copy.is_finalizable());
        assert!(copy.get().attributes_map().is_some());
    }

    #[test]
    fn test_copy_integer() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy
            .allocator
            .allocate_without_prototype(object_value::integer(5));

        let copy = dummy.copy_object(pointer).unwrap();

        assert!(copy.get().value.is_integer());
        assert_eq!(copy.integer_value().unwrap(), 5);
    }

    #[test]
    fn test_copy_float() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy
            .allocator
            .allocate_without_prototype(object_value::float(2.5));

        let copy = dummy.copy_object(pointer).unwrap();

        assert!(copy.get().value.is_float());
        assert_eq!(copy.get().value.as_float().unwrap(), 2.5);
    }

    #[test]
    fn test_copy_string() {
        let mut dummy = DummyAllocator::new();
        let pointer = dummy
            .allocator
            .allocate_without_prototype(object_value::string("a".to_string()));

        let copy = dummy.copy_object(pointer).unwrap();

        assert!(copy.get().value.is_string());
        assert_eq!(copy.string_value().unwrap().as_slice(), "a");
    }

    #[test]
    fn test_copy_array() {
        let mut dummy = DummyAllocator::new();
        let ptr1 = dummy.allocator.allocate_empty();
        let ptr2 = dummy.allocator.allocate_empty();
        let array = dummy
            .allocator
            .allocate_without_prototype(object_value::array(vec![ptr1, ptr2]));

        let copy = dummy.copy_object(array).unwrap();

        assert!(copy.get().value.is_array());
        assert_eq!(copy.get().value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_copy_block() {
        let mut dummy = DummyAllocator::new();
        let state = state();
        let name = state.intern_string("a".to_string());
        let path = state.intern_string("a.inko".to_string());
        let cc = CompiledCode::new(name, path, 1, Vec::new());
        let module = Module::new(name, cc, Vec::new());
        let block =
            Block::new(module.code(), None, ObjectPointer::integer(1), &module);

        let ptr = dummy
            .allocator
            .allocate_without_prototype(object_value::block(block));

        let copy = dummy.copy_object(ptr).unwrap();

        assert!(copy.get().value.is_block());
    }

    #[test]
    fn test_copy_binding() {
        let mut dummy = DummyAllocator::new();

        let local1 = dummy
            .allocator
            .allocate_without_prototype(object_value::float(15.0));

        let local2 = dummy
            .allocator
            .allocate_without_prototype(object_value::float(20.0));

        let receiver = dummy
            .allocator
            .allocate_without_prototype(object_value::float(12.0));

        let binding1 = Binding::new(1, ObjectPointer::integer(1), None);
        let binding2 = Binding::new(1, receiver, Some(binding1.clone()));

        binding1.set_local(0, local1);
        binding2.set_local(0, local2);

        let binding_ptr = dummy
            .allocator
            .allocate_without_prototype(object_value::binding(binding2));

        let binding_copy_ptr = dummy.copy_object(binding_ptr).unwrap();
        let binding_copy_obj = binding_copy_ptr.get();

        let binding_copy = binding_copy_obj.value.as_binding().unwrap();
        let parent_copy = binding_copy.parent().clone().unwrap();

        assert!(binding_copy.parent().is_some());
        assert_eq!(binding_copy.receiver().float_value().unwrap(), 12.0);

        let local1_copy = binding_copy.get_local(0);
        let local2_copy = parent_copy.get_local(0);

        assert!(local1 != local1_copy);
        assert!(local2 != local2_copy);

        assert_eq!(local1_copy.float_value().unwrap(), 20.0);
        assert_eq!(local2_copy.float_value().unwrap(), 15.0);
    }
}
