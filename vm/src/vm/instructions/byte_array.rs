//! VM functions for working with Inko byte arrays.
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{ByteArray, UnsignedInt};
use crate::runtime_error::RuntimeError;
use crate::scheduler::process_worker::ProcessWorker;
use crate::slicing;
use crate::vm::state::State;

#[inline(always)]
pub fn allocate(
    state: &State,
    worker: &mut ProcessWorker,
    generator: GeneratorPointer,
    start_reg: u16,
    amount: u16,
) -> Pointer {
    let mut values = Vec::with_capacity(amount as usize);

    for value in generator.context.get_registers(start_reg, amount) {
        values.push(unsafe { UnsignedInt::read(*value) } as u8);
    }

    ByteArray::alloc(
        worker.allocator(),
        state.permanent_space.byte_array_class(),
        values,
    )
}

#[inline(always)]
pub fn set(
    _: &State,
    array_ptr: Pointer,
    index_ptr: Pointer,
    value_ptr: Pointer,
) -> Result<(), RuntimeError> {
    let byte_array = unsafe { array_ptr.get_mut::<ByteArray>() };
    let bytes = byte_array.value_mut();
    let index = slicing::slice_index_to_usize(index_ptr, bytes.len());
    let value = unsafe { UnsignedInt::read(value_ptr) } as u8;

    if index > bytes.len() {
        return Err(RuntimeError::out_of_bounds(index));
    }

    if index == bytes.len() {
        bytes.push(value);
    } else {
        unsafe {
            *bytes.get_unchecked_mut(index) = value;
        }
    }

    Ok(())
}

#[inline(always)]
pub fn get(
    array_ptr: Pointer,
    index_ptr: Pointer,
) -> Result<Pointer, RuntimeError> {
    let byte_array = unsafe { array_ptr.get::<ByteArray>() };
    let bytes = byte_array.value();
    let index = slicing::slice_index_to_usize(index_ptr, bytes.len());

    bytes
        .get(index)
        .map(|byte| Pointer::int(*byte as i64))
        .ok_or_else(|| RuntimeError::out_of_bounds(index))
}

#[inline(always)]
pub fn remove(
    array_ptr: Pointer,
    index_ptr: Pointer,
) -> Result<Pointer, RuntimeError> {
    let byte_array = unsafe { array_ptr.get_mut::<ByteArray>() };
    let bytes = byte_array.value_mut();
    let index = slicing::slice_index_to_usize(index_ptr, bytes.len());

    if index >= bytes.len() {
        Err(RuntimeError::out_of_bounds(index))
    } else {
        Ok(Pointer::int(bytes.remove(index) as i64))
    }
}

#[inline(always)]
pub fn length(
    state: &State,
    worker: &mut ProcessWorker,
    array_ptr: Pointer,
) -> Pointer {
    let byte_array = unsafe { array_ptr.get::<ByteArray>() };
    let bytes = byte_array.value();

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.int_class(),
        bytes.len() as u64,
    )
}

#[inline(always)]
pub fn equals(
    state: &State,
    compare_ptr: Pointer,
    compare_with_ptr: Pointer,
) -> Pointer {
    let compare = unsafe { compare_ptr.get::<ByteArray>() };
    let compare_with = unsafe { compare_with_ptr.get::<ByteArray>() };

    if compare.value() == compare_with.value() {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}
