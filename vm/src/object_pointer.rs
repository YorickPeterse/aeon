// This lint is disabled here as not passing pointers by reference could
// potentially result in forwarding pointers not working properly.
#![cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]

use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use std::f32;
use std::f64;
use std::fmt;
use std::hash::{Hash, Hasher as HasherTrait};
use std::i16;
use std::i32;
use std::i64;
use std::i8;
use std::process::Child;
use std::ptr;
use std::u16;
use std::u32;
use std::u64;
use std::u8;
use std::usize;

use crate::arc_without_weak::ArcWithoutWeak;
use crate::binding::RcBinding;
use crate::block::Block;
use crate::external_functions::ExternalFunction;
use crate::ffi::{Library, Pointer, RcFunction};
use crate::file::File;
use crate::generator::RcGenerator;
use crate::hasher::Hasher;
use crate::immix::block;
use crate::immix::bucket::{MAILBOX, MATURE, PERMANENT};
use crate::immix::bytemap::Bytemap;
use crate::immix::local_allocator::YOUNG_MAX_AGE;
use crate::immutable_string::ImmutableString;
use crate::module::Module;
use crate::object::{Object, ObjectStatus, FORWARDED_BIT};
use crate::object_value::ObjectValue;
use crate::process::{FutureConsumer, RcProcess};
use crate::socket::Socket;
use crate::tagged_pointer::TaggedPointer;
use crate::vm::state::RcState;

/// Defines a method for getting the value of an object as a given type.
macro_rules! def_value_getter {
    ($name: ident, $getter: ident, $as_type: ident, $ok_type: ty) => {
        pub fn $name(&self) -> Result<$ok_type, String> {
            if self.is_tagged_integer() {
                Err(format!(
                    "ObjectPointer::{}() called on a tagged integer",
                    stringify!($as_type)
                ))
            } else {
                self.$getter().value.$as_type()
            }
        }
    };
}

macro_rules! def_integer_value_getter {
    ($name: ident, $kind: ident, $error_name: expr) => {
        pub fn $name(&self) -> Result<$kind, String> {
            let int_val = self.integer_value()?;

            if int_val < i64::from($kind::MIN)
                || int_val > i64::from($kind::MAX)
            {
                Err(format!(
                    "{} can't be converted to a {}",
                    int_val, $error_name
                ))
            } else {
                Ok(int_val as $kind)
            }
        }
    };
}

/// The minimum integer value that can be stored as a tagged integer.
pub const MIN_INTEGER: i64 = i64::MIN >> 1;

/// The maximum integer value that can be stored as a tagged integer.
pub const MAX_INTEGER: i64 = i64::MAX >> 1;

pub type RawObjectPointer = *mut Object;

/// A pointer to an object managed by the GC.
#[derive(Clone, Copy)]
pub struct ObjectPointer {
    /// The underlying tagged pointer. This pointer can have the following last
    /// two bits set:
    ///
    /// * 00: the pointer is a regular pointer
    /// * 01: the pointer is a tagged integer
    pub raw: TaggedPointer<Object>,
}

unsafe impl Send for ObjectPointer {}
unsafe impl Sync for ObjectPointer {}

/// A pointer to a object pointer. This wrapper is necessary to allow sharing
/// *const ObjectPointer pointers between threads.
#[derive(Clone)]
pub struct ObjectPointerPointer {
    pub raw: *const ObjectPointer,
}

unsafe impl Send for ObjectPointerPointer {}
unsafe impl Sync for ObjectPointerPointer {}

/// The bit to set for tagged integers.
pub const INTEGER_BIT: usize = 0;

/// Returns the BlockHeader of the given pointer.
fn block_header_of<'a>(
    pointer: RawObjectPointer,
) -> &'a mut block::BlockHeader {
    let addr = (pointer as isize & block::OBJECT_BYTEMAP_MASK) as usize;

    unsafe {
        let ptr = addr as *mut block::BlockHeader;

        &mut *ptr
    }
}

impl ObjectPointer {
    pub fn new(pointer: RawObjectPointer) -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::new(pointer),
        }
    }

    /// Creates a new tagged integer.
    pub fn integer(value: i64) -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::with_bit(
                (value << 1) as RawObjectPointer,
                INTEGER_BIT,
            ),
        }
    }

    pub fn byte(value: u8) -> ObjectPointer {
        Self::integer(i64::from(value))
    }

    /// Returns `true` if the given unsigned integer is too large for a tagged
    /// pointer.
    pub fn unsigned_integer_too_large(value: u64) -> bool {
        value > MAX_INTEGER as u64
    }

    /// Returns `true` if the given unsigned integer should be allocated as a
    /// big integer.
    pub fn unsigned_integer_as_big_integer(value: u64) -> bool {
        value > i64::MAX as u64
    }

    /// Returns `true` if the given value is too large for a tagged pointer.
    pub fn integer_too_large(value: i64) -> bool {
        value < MIN_INTEGER || value > MAX_INTEGER
    }

    /// Creates a new null pointer.
    pub fn null() -> ObjectPointer {
        ObjectPointer {
            raw: TaggedPointer::null(),
        }
    }

    /// Returns true if the current pointer points to a forwarded object.
    #[inline(always)]
    pub fn is_forwarded(&self) -> bool {
        self.get().is_forwarded()
    }

    /// Returns the status of the object.
    #[inline(always)]
    pub fn status(&mut self) -> ObjectStatus {
        if self.is_forwarded() {
            return ObjectStatus::Resolve;
        }

        let block = self.block();

        // If an object resides on a fragmented block _and_ needs to be
        // promoted, we can just promote it right away; instead of first
        // evacuating it and _then_ promoting it.
        //
        // If we instead evacuate such an object it may end up surviving too
        // many collections before being promoted.
        if block.bucket().unwrap().age == YOUNG_MAX_AGE {
            return if self.get_mut().mark_for_forward() {
                ObjectStatus::Promote
            } else {
                ObjectStatus::PendingMove
            };
        }

        if block.is_fragmented() {
            return if self.get_mut().mark_for_forward() {
                ObjectStatus::Evacuate
            } else {
                ObjectStatus::PendingMove
            };
        }

        ObjectStatus::OK
    }

    /// Replaces the current pointer with a pointer to the forwarded object.
    pub fn resolve_forwarding_pointer(&mut self) {
        let raw_attrs = self.get().attributes;

        // It's possible that between a previous check and calling this method
        // the pointer has already been resolved. In this case we should _not_
        // try to resolve anything as we'd end up storing the address to the
        // target objects' _prototype_, and not the target object itself.
        if raw_attrs.bit_is_set(FORWARDED_BIT) {
            self.raw =
                TaggedPointer::new(raw_attrs.untagged() as RawObjectPointer);
        }
    }

    /// Returns an immutable reference to the Object.
    #[inline(always)]
    pub fn get(&self) -> &Object {
        self.raw
            .as_ref()
            .expect("ObjectPointer::get() called on a NULL pointer")
    }

    /// Returns a mutable reference to the Object.
    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn get_mut(&self) -> &mut Object {
        self.raw
            .as_mut()
            .expect("ObjectPointer::get_mut() called on a NULL pointer")
    }

    /// Returns true if the current pointer is a null pointer.
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.raw.raw as usize == 0x0
    }

    /// Returns true if the current pointer points to a permanent object.
    pub fn is_permanent(&self) -> bool {
        self.is_tagged_integer()
            || self.block().bucket().unwrap().age == PERMANENT
    }

    /// Returns true if the current pointer points to a mature object.
    pub fn is_mature(&self) -> bool {
        !self.is_tagged_integer()
            && self.block().bucket().unwrap().age == MATURE
    }

    /// Returns true if the current pointer points to a mailbox object.
    pub fn is_mailbox(&self) -> bool {
        !self.is_tagged_integer()
            && self.block().bucket().unwrap().age == MAILBOX
    }

    /// Returns true if the current pointer points to a young object.
    pub fn is_young(&self) -> bool {
        !self.is_tagged_integer()
            && self.block().bucket().unwrap().age <= YOUNG_MAX_AGE
    }

    /// Marks the current object and its line.
    ///
    /// As this method is called often during collection, this method refers to
    /// `self.raw` only once and re-uses the pointer. This ensures there are no
    /// race conditions when determining the object/line indexes, and reduces
    /// the overhead of having to call `self.raw.untagged()` multiple times.
    pub fn mark(&self) {
        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();

        let object_index = block.object_index_of_pointer(pointer);
        let line_index = block.line_index_of_pointer(pointer);

        block.marked_objects_bytemap.set(object_index);
        block.used_lines_bytemap.set(line_index);
    }

    /// Unmarks the current object.
    ///
    /// The line mark state is not changed.
    pub fn unmark(&self) {
        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();

        let object_index = block.object_index_of_pointer(pointer);

        block.marked_objects_bytemap.unset(object_index);
    }

    /// Returns true if the current object is marked.
    ///
    /// This method *must not* use any methods that also call
    /// `self.raw.untagged()` as doing so will lead to race conditions producing
    /// incorrect object/line indexes. This can happen when one tried checks if
    /// an object is marked while another thread is updating the pointer's
    /// address (e.g. after evacuating the underlying object).
    pub fn is_marked(&self) -> bool {
        if self.is_tagged_integer() {
            return true;
        }

        let pointer = self.raw.untagged();
        let header = block_header_of(pointer);
        let block = header.block_mut();
        let index = block.object_index_of_pointer(pointer);

        block.marked_objects_bytemap.is_set(index)
    }

    /// Marks the object this pointer points to as being remembered in a
    /// remembered set.
    pub fn mark_as_remembered(&self) {
        self.get_mut().mark_as_remembered();
    }

    /// Returns `true` if the object this pointer points to has been remembered
    /// in a remembered set.
    pub fn is_remembered(&self) -> bool {
        self.get().is_remembered()
    }

    /// Returns a mutable reference to the block this pointer belongs to.
    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn block_mut(&self) -> &mut block::Block {
        block_header_of(self.raw.untagged()).block_mut()
    }

    /// Returns an immutable reference to the block this pointer belongs to.
    #[inline(always)]
    pub fn block(&self) -> &block::Block {
        block_header_of(self.raw.untagged()).block()
    }

    /// Returns true if the object should be finalized.
    pub fn is_finalizable(&self) -> bool {
        !self.is_tagged_integer() && self.get().is_finalizable()
    }

    /// Finalizes the underlying object, if needed.
    pub fn finalize(&self) {
        if !self.is_finalizable() {
            return;
        }

        unsafe {
            ptr::drop_in_place(self.raw.raw);

            // We zero out the memory so future finalize() calls for the same
            // object (before other allocations take place) don't try to free
            // the memory again.
            ptr::write_bytes(self.raw.raw, 0, 1);
        }
    }

    /// Adds an attribute to the object this pointer points to.
    pub fn add_attribute(
        &self,
        process: &RcProcess,
        name: ObjectPointer,
        attr: ObjectPointer,
    ) {
        self.get_mut().add_attribute(name, attr);

        process.write_barrier(*self, attr);
    }

    /// Looks up an attribute.
    pub fn lookup_attribute(
        &self,
        state: &RcState,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            state.integer_prototype.get().lookup_attribute(name)
        } else {
            self.get().lookup_attribute(name)
        }
    }

    /// Looks up an attribute without walking the prototype chain.
    pub fn lookup_attribute_in_self(
        &self,
        state: &RcState,
        name: ObjectPointer,
    ) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            state.integer_prototype.get().lookup_attribute_in_self(name)
        } else {
            self.get().lookup_attribute_in_self(name)
        }
    }

    pub fn attributes(&self) -> Vec<ObjectPointer> {
        if self.is_tagged_integer() {
            Vec::new()
        } else {
            self.get().attributes()
        }
    }

    pub fn attribute_names(&self) -> Vec<ObjectPointer> {
        if self.is_tagged_integer() {
            Vec::new()
        } else {
            self.get().attribute_names()
        }
    }

    pub fn set_prototype(&self, proto: ObjectPointer) {
        self.get_mut().set_prototype(proto);
    }

    pub fn prototype(&self, state: &RcState) -> Option<ObjectPointer> {
        if self.is_tagged_integer() {
            Some(state.integer_prototype)
        } else {
            self.get().prototype()
        }
    }

    /// Returns a pointer to this pointer.
    pub fn pointer(&self) -> ObjectPointerPointer {
        ObjectPointerPointer::new(self)
    }

    pub fn is_tagged_integer(&self) -> bool {
        self.raw.bit_is_set(INTEGER_BIT)
    }

    pub fn is_string(&self) -> bool {
        if self.is_tagged_integer() {
            false
        } else {
            self.get().value.is_string()
        }
    }

    pub fn is_interned_string(&self) -> bool {
        if self.is_tagged_integer() {
            false
        } else {
            self.get().value.is_interned_string()
        }
    }

    pub fn is_integer(&self) -> bool {
        self.is_tagged_integer() || self.get().value.is_integer()
    }

    pub fn is_float(&self) -> bool {
        self.float_value().is_ok()
    }

    pub fn is_bigint(&self) -> bool {
        if self.is_integer() {
            false
        } else {
            self.get().value.is_bigint()
        }
    }

    pub fn is_zero_integer(&self) -> bool {
        if self.is_integer() {
            self.integer_value().unwrap().is_zero()
        } else if self.is_bigint() {
            self.bigint_value().unwrap().is_zero()
        } else {
            false
        }
    }

    pub fn is_in_u32_range(&self) -> bool {
        if let Ok(integer) = self.integer_value() {
            integer >= i64::from(u32::MIN) && integer <= i64::from(u32::MAX)
        } else {
            false
        }
    }

    pub fn is_in_i32_range(&self) -> bool {
        if let Ok(integer) = self.integer_value() {
            integer >= i64::from(i32::MIN) && integer <= i64::from(i32::MAX)
        } else {
            false
        }
    }

    pub fn is_immutable(&self) -> bool {
        self.is_tagged_integer() || self.get().value.is_immutable()
    }

    pub fn integer_value(&self) -> Result<i64, String> {
        if self.is_tagged_integer() {
            Ok(self.raw.raw as i64 >> 1)
        } else if let Ok(num) = self.get().value.as_integer() {
            Ok(num)
        } else {
            Err(
                "ObjectPointer::integer_value() called on a non integer object"
                    .to_string(),
            )
        }
    }

    pub fn integer_to_usize(&self) -> Result<usize, String> {
        let int_val = self.integer_value()?;

        if int_val < 0 || int_val as u64 > usize::MAX as u64 {
            Err(format!(
                "{} is too big to convert to an unsigned integer",
                int_val
            ))
        } else {
            Ok(int_val as usize)
        }
    }

    pub fn bigint_to_usize(&self) -> Result<usize, String> {
        let int_val = self.bigint_value()?;

        int_val.to_usize().ok_or_else(|| {
            format!("{} is too big to convert to an unsigned integer", int_val)
        })
    }

    pub fn usize_value(&self) -> Result<usize, String> {
        if self.is_bigint() {
            self.bigint_to_usize()
        } else {
            self.integer_to_usize()
        }
    }

    def_integer_value_getter!(i8_value, i8, "signed 8 bits integer");
    def_integer_value_getter!(i16_value, i16, "signed 16 bits integer");
    def_integer_value_getter!(i32_value, i32, "signed 32 bits integer");

    def_integer_value_getter!(u8_value, u8, "unsigned 8 bits integer");
    def_integer_value_getter!(u16_value, u16, "unsigned 16 bits integer");
    def_integer_value_getter!(u32_value, u32, "unsigned 32 bits integer");

    pub fn u64_value(&self) -> Result<u64, String> {
        self.usize_value().map(|num| num as u64)
    }

    pub fn f32_value(&self) -> Result<f32, String> {
        let value = self.float_value()?;

        if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
            Err(format!(
                "{} can't be converted to a 32 bits floating point",
                value
            ))
        } else {
            Ok(value as f32)
        }
    }

    pub fn hash_object(&self, hasher: &mut Hasher) -> Result<(), String> {
        if self.is_tagged_integer() {
            hasher.write_integer(self.integer_value()?);
        } else {
            let value_ref = self.get();

            match value_ref.value {
                ObjectValue::Float(val) => hasher.write_float(val),
                ObjectValue::Integer(val) => hasher.write_integer(val),
                ObjectValue::BigInt(ref val) => hasher.write_bigint(val),
                ObjectValue::String(ref val) => hasher.write_string(val),
                ObjectValue::InternedString(ref val) => {
                    hasher.write_string(val)
                }
                _ => {
                    hasher.write_unsigned_integer(self.raw.untagged() as usize)
                }
            };
        }

        Ok(())
    }

    def_value_getter!(float_value, get, as_float, f64);
    def_value_getter!(string_value, get, as_string, &ImmutableString);

    def_value_getter!(array_value, get, as_array, &Vec<ObjectPointer>);
    def_value_getter!(
        array_value_mut,
        get_mut,
        as_array_mut,
        &mut Vec<ObjectPointer>
    );

    def_value_getter!(file_value, get, as_file, &File);
    def_value_getter!(file_value_mut, get_mut, as_file_mut, &mut File);

    def_value_getter!(block_value, get, as_block, &Block);
    def_value_getter!(binding_value, get, as_binding, RcBinding);
    def_value_getter!(bigint_value, get, as_bigint, &BigInt);
    def_value_getter!(hasher_value_mut, get_mut, as_hasher_mut, &mut Hasher);
    def_value_getter!(hasher_value, get, as_hasher, &Hasher);

    def_value_getter!(byte_array_value, get, as_byte_array, &Vec<u8>);
    def_value_getter!(
        byte_array_value_mut,
        get_mut,
        as_byte_array_mut,
        &mut Vec<u8>
    );

    def_value_getter!(library_value, get, as_library, &Library);
    def_value_getter!(function_value, get, as_function, &RcFunction);
    def_value_getter!(pointer_value, get, as_pointer, Pointer);
    def_value_getter!(process_value, get, as_process, &RcProcess);
    def_value_getter!(socket_value, get, as_socket, &Socket);

    def_value_getter!(socket_value_mut, get_mut, as_socket_mut, &mut Socket);
    def_value_getter!(module_value, get, as_module, &ArcWithoutWeak<Module>);
    def_value_getter!(
        module_value_mut,
        get_mut,
        as_module_mut,
        &mut ArcWithoutWeak<Module>
    );

    def_value_getter!(generator_value, get, as_generator, &RcGenerator);
    def_value_getter!(
        external_function_value,
        get,
        as_external_function,
        ExternalFunction
    );
    def_value_getter!(command_value, get, as_command, &Child);
    def_value_getter!(command_value_mut, get_mut, as_command_mut, &mut Child);
    def_value_getter!(future_value, get, as_future, &FutureConsumer);

    /// Atomically loads the underlying pointer, returning a new ObjectPointer.
    pub fn atomic_load(&self) -> Self {
        ObjectPointer {
            raw: TaggedPointer::new(self.raw.atomic_load()),
        }
    }

    pub fn integer_to_string(&self) -> Result<String, String> {
        let string = if self.is_bigint() {
            self.bigint_value()?.to_string()
        } else {
            self.integer_value()?.to_string()
        };

        Ok(string)
    }
}

impl ObjectPointerPointer {
    #[cfg_attr(feature = "cargo-clippy", allow(trivially_copy_pass_by_ref))]
    pub fn new(pointer: &ObjectPointer) -> ObjectPointerPointer {
        ObjectPointerPointer {
            raw: pointer as *const ObjectPointer,
        }
    }

    #[inline(always)]
    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn get_mut(&self) -> &mut ObjectPointer {
        unsafe { &mut *(self.raw as *mut ObjectPointer) }
    }

    #[inline(always)]
    pub fn get(&self) -> &ObjectPointer {
        unsafe { &*self.raw }
    }
}

impl PartialEq for ObjectPointer {
    fn eq(&self, other: &ObjectPointer) -> bool {
        self.raw == other.raw
    }
}

impl Eq for ObjectPointer {}

impl Hash for ObjectPointer {
    fn hash<H: HasherTrait>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl fmt::Debug for ObjectPointer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "ObjectPointer({:?})", self.raw.raw)
    }
}

impl PartialEq for ObjectPointerPointer {
    fn eq(&self, other: &ObjectPointerPointer) -> bool {
        self.raw == other.raw
    }
}

impl Eq for ObjectPointerPointer {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::i128;

    use crate::config::Config;
    use crate::immix::block::Block;
    use crate::immix::bucket::{Bucket, MAILBOX, MATURE, PERMANENT};
    use crate::immix::bytemap::Bytemap;
    use crate::immix::global_allocator::GlobalAllocator;
    use crate::immix::local_allocator::LocalAllocator;
    use crate::object::{Object, ObjectStatus};
    use crate::object_value::{self, ObjectValue};
    use crate::vm::state::State;

    fn fake_raw_pointer() -> RawObjectPointer {
        0x4 as RawObjectPointer
    }

    fn object_pointer_for(object: &Object) -> ObjectPointer {
        ObjectPointer::new(object as *const Object as RawObjectPointer)
    }

    fn local_allocator() -> LocalAllocator {
        LocalAllocator::new(GlobalAllocator::with_rc(), &Config::new())
    }

    fn buckets_for_all_ages() -> (Bucket, Bucket, Bucket, Bucket) {
        let young = Bucket::with_age(0);
        let mature = Bucket::with_age(MATURE);
        let mailbox = Bucket::with_age(MAILBOX);
        let permanent = Bucket::with_age(PERMANENT);

        (young, mature, mailbox, permanent)
    }

    fn allocate_in_bucket(bucket: &mut Bucket) -> ObjectPointer {
        if bucket.blocks.is_empty() {
            bucket.add_block(Block::boxed());
        }

        let raw_pointer =
            bucket.current_block().unwrap().request_pointer().unwrap();

        Object::new(ObjectValue::None).write_to(raw_pointer)
    }

    #[test]
    fn test_object_pointer_new() {
        let pointer = ObjectPointer::new(fake_raw_pointer());

        assert_eq!(pointer.raw.raw as usize, 0x4);
    }

    #[test]
    fn test_object_pointer_null() {
        let pointer = ObjectPointer::null();

        assert_eq!(pointer.raw.raw as usize, 0x0);
    }

    #[test]
    fn test_byte() {
        let pointer = ObjectPointer::byte(5);

        assert_eq!(pointer.integer_value().unwrap(), 5);
    }

    #[test]
    fn test_object_pointer_is_forwarded_with_regular_pointer() {
        let object = Object::new(ObjectValue::None);
        let pointer = object_pointer_for(&object);

        assert_eq!(pointer.is_forwarded(), false);
    }

    #[test]
    fn test_object_pointer_is_forwarded_with_forwarding_pointer() {
        let mut source = Object::new(ObjectValue::None);
        let target = Object::new(ObjectValue::None);
        let target_ptr = object_pointer_for(&target);

        source.forward_to(target_ptr);

        let source_ptr = object_pointer_for(&source);

        assert_eq!(source_ptr.is_forwarded(), true);
    }

    #[test]
    fn test_object_pointer_status() {
        let mut allocator = local_allocator();
        let mut pointer = allocator.allocate_empty();

        assert_eq!(pointer.status(), ObjectStatus::OK);
    }

    #[test]
    fn test_object_pointer_status_promote() {
        let mut allocator = local_allocator();
        let mut pointer = allocator.allocate_empty();
        let bucket = pointer.block_mut().bucket_mut().unwrap();

        bucket.increment_age();
        bucket.increment_age();

        assert_eq!(pointer.status(), ObjectStatus::Promote);
        assert_eq!(pointer.status(), ObjectStatus::PendingMove);
    }

    #[test]
    fn test_object_pointer_status_promote_from_fragmented_block() {
        let mut allocator = local_allocator();
        let mut pointer = allocator.allocate_empty();
        let bucket = pointer.block_mut().bucket_mut().unwrap();

        bucket.increment_age();
        bucket.increment_age();
        pointer.block_mut().set_fragmented();

        assert_eq!(pointer.status(), ObjectStatus::Promote);
        assert_eq!(pointer.status(), ObjectStatus::PendingMove);
    }

    #[test]
    fn test_object_pointer_status_fragmented() {
        let mut allocator = local_allocator();
        let mut pointer = allocator.allocate_empty();
        let pointer2 = allocator.allocate_empty();

        pointer.block_mut().set_fragmented();

        assert_eq!(pointer.status(), ObjectStatus::Evacuate);
        assert_eq!(pointer.status(), ObjectStatus::PendingMove);

        pointer.get_mut().forward_to(pointer2);

        assert_eq!(pointer.status(), ObjectStatus::Resolve);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer() {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Float(2.0));

        object.forward_to(proto_pointer);

        let mut pointer = object_pointer_for(&object);

        pointer.resolve_forwarding_pointer();

        assert!(pointer == proto_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_concurrently() {
        // The object to forward to.
        let mut target_object = Object::new(ObjectValue::None);
        let target_proto = Object::new(ObjectValue::None);
        let target_pointer = object_pointer_for(&target_object);

        // For this test the target object must have a prototype. This ensures
        // resolving the pointer doesn't return early due to there not being a
        // prototype.
        target_object.set_prototype(object_pointer_for(&target_proto));

        // The object that is being forwarded.
        let mut forwarded_object = Object::new(ObjectValue::None);

        forwarded_object.forward_to(target_pointer);

        let forwarded_pointer = object_pointer_for(&forwarded_object);

        let ptr_ptr1 = forwarded_pointer.pointer();
        let ptr_ptr2 = forwarded_pointer.pointer();

        // "Simulate" two threads concurrently resolving the same pointer.
        let ptr1 = ptr_ptr1.get_mut();
        let ptr2 = ptr_ptr2.get_mut();

        ptr1.resolve_forwarding_pointer();
        ptr2.resolve_forwarding_pointer();

        assert!(*ptr1 == target_pointer);
        assert!(*ptr2 == target_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_in_vector() {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Float(2.0));

        object.forward_to(proto_pointer);

        let mut pointers = vec![object_pointer_for(&object)];

        pointers.get_mut(0).unwrap().resolve_forwarding_pointer();

        assert!(pointers[0] == proto_pointer);
    }

    #[test]
    fn test_object_pointer_resolve_forwarding_pointer_in_vector_with_pointer_pointers(
    ) {
        let proto = Object::new(ObjectValue::None);
        let proto_pointer = object_pointer_for(&proto);
        let mut object = Object::new(ObjectValue::Float(2.0));

        object.forward_to(proto_pointer);

        let mut pointers = vec![object_pointer_for(&object)];
        let mut pointer_pointers = vec![&mut pointers[0] as *mut ObjectPointer];

        let ptr_ref = unsafe { &mut *pointer_pointers[0] };

        ptr_ref.resolve_forwarding_pointer();

        assert!(pointers[0] == proto_pointer);
    }

    #[test]
    fn test_object_pointer_get_get_mut() {
        let object = Object::new(ObjectValue::Float(2.0));
        let pointer = object_pointer_for(&object);

        // Object doesn't implement PartialEq/Eq so we can't compare references,
        // thus we'll just test if we get something somewhat correct-ish.
        assert!(pointer.get().value.is_float());
        assert!(pointer.get_mut().value.is_float());
    }

    #[test]
    fn test_object_pointer_is_null_with_null_pointer() {
        let pointer = ObjectPointer::null();

        assert!(pointer.is_null());
    }

    #[test]
    fn test_object_pointer_is_null_with_regular_pointer() {
        let pointer = ObjectPointer::new(fake_raw_pointer());

        assert_eq!(pointer.is_null(), false);
    }

    #[test]
    fn test_object_pointer_is_permanent() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_permanent(), false);
        assert_eq!(mature_ptr.is_permanent(), false);
        assert_eq!(mailbox_ptr.is_permanent(), false);
        assert_eq!(permanent_ptr.is_permanent(), true);
    }

    #[test]
    fn test_object_pointer_is_young() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_young(), true);
        assert_eq!(mature_ptr.is_young(), false);
        assert_eq!(mailbox_ptr.is_young(), false);
        assert_eq!(permanent_ptr.is_young(), false);
    }

    #[test]
    fn test_object_pointer_is_mature() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_mature(), false);
        assert_eq!(mature_ptr.is_mature(), true);
        assert_eq!(mailbox_ptr.is_mature(), false);
        assert_eq!(permanent_ptr.is_mature(), false);
    }

    #[test]
    fn test_object_pointer_is_mailbox() {
        let (mut young, mut mature, mut mailbox, mut permanent) =
            buckets_for_all_ages();

        let young_ptr = allocate_in_bucket(&mut young);
        let mature_ptr = allocate_in_bucket(&mut mature);
        let mailbox_ptr = allocate_in_bucket(&mut mailbox);
        let permanent_ptr = allocate_in_bucket(&mut permanent);

        assert_eq!(young_ptr.is_mailbox(), false);
        assert_eq!(mature_ptr.is_mailbox(), false);
        assert_eq!(mailbox_ptr.is_mailbox(), true);
        assert_eq!(permanent_ptr.is_mailbox(), false);
    }

    #[test]
    fn test_object_pointer_is_marked_with_unmarked_object() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        assert_eq!(pointer.is_marked(), false);
    }

    #[test]
    fn test_object_pointer_is_marked_with_marked_object() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.mark();

        assert!(pointer.is_marked());
    }

    #[test]
    fn test_object_pointer_mark() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.mark();

        assert!(pointer.block().marked_objects_bytemap.is_set(4));
        assert!(pointer.block().used_lines_bytemap.is_set(1));
    }

    #[test]
    fn test_object_pointer_unmark() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.mark();
        pointer.unmark();

        assert_eq!(pointer.block().marked_objects_bytemap.is_set(4), false);

        assert!(pointer.block().used_lines_bytemap.is_set(1));
    }

    #[test]
    fn test_object_pointer_mark_as_remembered() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        assert_eq!(pointer.is_remembered(), false);

        pointer.mark_as_remembered();

        assert!(pointer.is_remembered());
    }

    #[test]
    fn test_object_pointer_block_mut() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.block_mut();
    }

    #[test]
    fn test_object_pointer_block() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        pointer.block();
    }

    #[test]
    fn test_object_pointer_pointer() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();

        let raw_pointer = pointer.pointer();

        assert!(*raw_pointer.get() == pointer);

        // Using the raw pointer for any updates should result in the
        // ObjectPointer being updated properly.
        let reference = raw_pointer.get_mut();

        reference.raw.set_bit(0);

        assert!(pointer.raw.bit_is_set(0));
    }

    #[test]
    fn test_object_pointer_eq() {
        let mut allocator = local_allocator();
        let pointer1 = allocator.allocate_empty();
        let pointer2 = allocator.allocate_empty();

        assert!(pointer1 == pointer1);
        assert!(pointer1 != pointer2);
    }

    #[test]
    fn test_object_pointer_hashing() {
        let mut allocator = local_allocator();
        let pointer = allocator.allocate_empty();
        let mut set = HashSet::new();

        set.insert(pointer);

        assert!(set.contains(&pointer));
    }

    #[test]
    fn test_object_pointer_integer_too_large() {
        assert_eq!(ObjectPointer::integer_too_large(i64::MAX), true);
        assert_eq!(ObjectPointer::integer_too_large(MAX_INTEGER), false);
        assert_eq!(ObjectPointer::integer_too_large(5), false);
    }

    #[test]
    fn test_object_pointer_integer_value() {
        for i in 1..10 {
            assert_eq!(ObjectPointer::integer(i).integer_value().unwrap(), i);
        }
    }

    #[test]
    fn test_object_pointer_maximum_value() {
        let valid = ObjectPointer::integer(MAX_INTEGER);
        let invalid = ObjectPointer::integer(MAX_INTEGER + 1);

        assert_eq!(valid.integer_value().unwrap(), MAX_INTEGER);
        assert_eq!(invalid.integer_value().unwrap(), MIN_INTEGER);
    }

    #[test]
    fn test_object_pointer_minimum_value() {
        let valid = ObjectPointer::integer(MIN_INTEGER);
        let invalid = ObjectPointer::integer(MIN_INTEGER - 1);

        assert_eq!(valid.integer_value().unwrap(), MIN_INTEGER);
        assert_eq!(invalid.integer_value().unwrap(), MAX_INTEGER);
    }

    #[test]
    fn test_object_pointer_lookup_attribute_with_integer() {
        let state = State::with_rc(Config::new(), &[]);
        let ptr = ObjectPointer::integer(5);
        let name = state.intern_string("foo".to_string());
        let method = state.permanent_allocator.lock().allocate_empty();

        state
            .integer_prototype
            .get_mut()
            .add_attribute(name, method);

        assert!(ptr.lookup_attribute(&state, name).unwrap() == method);
    }

    #[test]
    fn test_object_pointer_lookup_attribute_in_self_with_integer() {
        let state = State::with_rc(Config::new(), &[]);
        let ptr = ObjectPointer::integer(5);
        let name = state.intern_string("foo".to_string());
        let method = state.permanent_allocator.lock().allocate_empty();

        state
            .integer_prototype
            .get_mut()
            .add_attribute(name, method);

        assert!(ptr.lookup_attribute_in_self(&state, name).unwrap() == method);
    }

    #[test]
    fn test_object_pointer_lookup_attribute_with_integer_without_attribute() {
        let state = State::with_rc(Config::new(), &[]);
        let ptr = ObjectPointer::integer(5);
        let name = state.intern_string("foo".to_string());

        assert!(ptr.lookup_attribute(&state, name).is_none());
    }

    #[test]
    fn test_object_pointer_lookup_attribute_with_object() {
        let state = State::with_rc(Config::new(), &[]);
        let ptr = state.permanent_allocator.lock().allocate_empty();
        let name = state.intern_string("foo".to_string());
        let value = state.permanent_allocator.lock().allocate_empty();

        ptr.get_mut().add_attribute(name, value);

        assert!(ptr.lookup_attribute(&state, name).unwrap() == value);
    }

    #[test]
    fn test_object_pointer_pointer_get_mut() {
        let ptr = ObjectPointer::new(fake_raw_pointer());
        let ptr_ptr = ptr.pointer();

        ptr_ptr.get_mut().raw.raw = 0x5 as RawObjectPointer;

        assert_eq!(ptr.raw.raw as usize, 0x5);
    }

    #[test]
    fn test_object_pointer_pointer_eq() {
        let ptr = ObjectPointer::new(fake_raw_pointer());
        let ptr_ptr1 = ptr.pointer();
        let ptr_ptr2 = ptr.pointer();

        assert!(ptr_ptr1 == ptr_ptr2);
    }

    #[test]
    fn test_is_immutable() {
        let state = State::with_rc(Config::new(), &[]);
        let name = state.intern_string("foo".to_string());

        assert!(name.is_immutable());
        assert!(ObjectPointer::integer(5).is_immutable());
    }

    #[test]
    fn test_is_finalizable() {
        let mut allocator = local_allocator();
        let ptr =
            allocator.allocate_without_prototype(ObjectValue::Integer(10));

        assert!(ptr.is_finalizable());
        assert_eq!(ObjectPointer::integer(5).is_finalizable(), false);
    }

    #[test]
    fn test_finalize() {
        let mut allocator = local_allocator();
        let ptr1 =
            allocator.allocate_without_prototype(ObjectValue::Integer(10));

        let ptr2 = allocator.allocate_empty();

        ptr1.get_mut().add_attribute(ptr2, ptr2);
        ptr1.finalize();

        let obj1 = ptr1.get();

        assert!(obj1.attributes.is_null());
    }

    #[test]
    fn test_usize_value() {
        let ptr = ObjectPointer::integer(5);

        assert_eq!(ptr.usize_value().unwrap(), 5);
    }

    #[test]
    fn test_integer_to_usize() {
        assert_eq!(ObjectPointer::integer(5).integer_to_usize().unwrap(), 5);
        assert!(ObjectPointer::integer(-5).integer_to_usize().is_err());
    }

    #[test]
    fn test_bigint_to_usize() {
        let mut alloc = local_allocator();
        let small = alloc
            .allocate_without_prototype(object_value::bigint(BigInt::from(5)));

        let big = alloc.allocate_without_prototype(object_value::bigint(
            BigInt::from(i128::MAX),
        ));

        assert_eq!(small.bigint_to_usize().unwrap(), 5);
        assert!(big.bigint_to_usize().is_err());
    }

    #[test]
    fn test_i8_value() {
        let small = ObjectPointer::integer(5);
        let large = ObjectPointer::integer(i8::MAX as i64 + 1);

        assert_eq!(small.i8_value().unwrap(), 5);
        assert!(large.i8_value().is_err());
    }

    #[test]
    fn test_i16_value() {
        let small = ObjectPointer::integer(5);
        let large = ObjectPointer::integer(i16::MAX as i64 + 1);

        assert_eq!(small.i16_value().unwrap(), 5);
        assert!(large.i16_value().is_err());
    }

    #[test]
    fn test_i32_value() {
        let small = ObjectPointer::integer(5);
        let large = ObjectPointer::integer(i32::MAX as i64 + 1);

        assert_eq!(small.i32_value().unwrap(), 5);
        assert!(large.i32_value().is_err());
    }

    #[test]
    fn test_u8_value() {
        let small = ObjectPointer::integer(5);
        let large = ObjectPointer::integer(u8::MAX as i64 + 1);

        assert_eq!(small.u8_value().unwrap(), 5);
        assert!(large.u8_value().is_err());
    }

    #[test]
    fn test_u16_value() {
        let small = ObjectPointer::integer(5);
        let large = ObjectPointer::integer(u16::MAX as i64 + 1);

        assert_eq!(small.u16_value().unwrap(), 5);
        assert!(large.u16_value().is_err());
    }

    #[test]
    fn test_u32_value() {
        let small = ObjectPointer::integer(5);
        let large = ObjectPointer::integer(u32::MAX as i64 + 1);

        assert_eq!(small.u32_value().unwrap(), 5);
        assert!(large.u32_value().is_err());
    }

    #[test]
    fn test_u64_value() {
        let mut alloc = local_allocator();

        let small = ObjectPointer::integer(5);
        let large = alloc.allocate_without_prototype(object_value::bigint(
            BigInt::from(u64::MAX) + 1,
        ));

        assert_eq!(small.u64_value().unwrap(), 5);
        assert!(large.u64_value().is_err());
    }

    #[test]
    fn test_f32_value() {
        let mut alloc = local_allocator();

        let small = alloc.allocate_without_prototype(object_value::float(1.5));
        let large =
            alloc.allocate_without_prototype(object_value::float(f64::MAX));

        assert_eq!(small.f32_value().unwrap(), 1.5);
        assert!(large.f32_value().is_err());
    }

    #[test]
    fn test_is_zero_integer_with_tagged_integers() {
        assert_eq!(ObjectPointer::integer(5).is_zero_integer(), false);
        assert!(ObjectPointer::integer(0).is_zero_integer());
    }

    #[test]
    fn test_is_zero_integer_with_heap_integers() {
        let mut alloc = local_allocator();
        let non_zero =
            alloc.allocate_without_prototype(object_value::integer(5));

        let zero = alloc.allocate_without_prototype(object_value::integer(0));

        assert_eq!(non_zero.is_zero_integer(), false);
        assert!(zero.is_zero_integer());
    }

    #[test]
    fn test_is_zero_integer_with_big_integers() {
        let mut alloc = local_allocator();
        let non_zero = alloc
            .allocate_without_prototype(object_value::bigint(BigInt::from(5)));

        let zero = alloc
            .allocate_without_prototype(object_value::bigint(BigInt::from(0)));

        assert_eq!(non_zero.is_zero_integer(), false);
        assert!(zero.is_zero_integer());
    }

    #[test]
    fn test_atomic_load() {
        let mut alloc = local_allocator();
        let pointer = alloc.allocate_empty();

        assert!(pointer.atomic_load() == pointer);
    }
}
