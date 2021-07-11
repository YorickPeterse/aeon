//! VM functions for working with Inko arrays.
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Array, Int};
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
    let values = generator.context.get_registers(start_reg, amount).to_vec();

    Array::alloc(
        worker.allocator(),
        state.permanent_space.array_class(),
        values,
    )
}

#[inline(always)]
pub fn set(
    array_ptr: Pointer,
    index_ptr: Pointer,
    value_ptr: Pointer,
) -> Result<(), RuntimeError> {
    let array = unsafe { array_ptr.get_mut::<Array>() };
    let vector = array.value_mut();
    let index = slicing::slice_index_to_usize(index_ptr, vector.len());

    if index > vector.len() {
        return Err(RuntimeError::out_of_bounds(index));
    }

    if index == vector.len() {
        vector.push(value_ptr);
    } else {
        unsafe {
            *vector.get_unchecked_mut(index) = value_ptr;
        }
    }

    Ok(())
}

#[inline(always)]
pub fn get(
    array_ptr: Pointer,
    index_ptr: Pointer,
) -> Result<Pointer, RuntimeError> {
    let array = unsafe { array_ptr.get::<Array>() };
    let vector = array.value();
    let index = slicing::slice_index_to_usize(index_ptr, vector.len());

    vector
        .get(index)
        .cloned()
        .ok_or_else(|| RuntimeError::out_of_bounds(index))
}

#[inline(always)]
pub fn remove(
    array_ptr: Pointer,
    index_ptr: Pointer,
) -> Result<Pointer, RuntimeError> {
    let array = unsafe { array_ptr.get_mut::<Array>() };
    let vector = array.value_mut();
    let index = slicing::slice_index_to_usize(index_ptr, vector.len());

    if index >= vector.len() {
        return Err(RuntimeError::out_of_bounds(index));
    }

    Ok(vector.remove(index))
}

#[inline(always)]
pub fn length(
    state: &State,
    worker: &mut ProcessWorker,
    pointer: Pointer,
) -> Pointer {
    let array = unsafe { pointer.get::<Array>() };
    let vector = array.value();

    Int::alloc(
        worker.allocator(),
        state.permanent_space.int_class(),
        vector.len() as i64,
    )
}
