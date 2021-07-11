//! VM functions for working with Inko objects.
use crate::execution_context::ExecutionContext;
use crate::indexes::{FieldIndex, MethodIndex};
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::Method;
use crate::mem::objects::{Class, ClassPointer, Object};
use crate::numeric::u32_from_u16_pair;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::state::State;

#[inline(always)]
pub fn allocate(worker: &mut ProcessWorker, class: Pointer) -> Pointer {
    Object::alloc(worker.allocator(), unsafe { ClassPointer::new(class) })
}

#[inline(always)]
pub fn get_field(receiver: Pointer, index: u16) -> Pointer {
    unsafe { receiver.get::<Object>().get(FieldIndex::new(index as u8)) }
}

#[inline(always)]
pub fn set_field(receiver: Pointer, index: u16, value: Pointer) {
    unsafe {
        receiver
            .get_mut::<Object>()
            .set(FieldIndex::new(index as u8), value);
    }
}

#[inline(always)]
pub fn get_class(state: &State, pointer: Pointer) -> Pointer {
    Class::of(&state.permanent_space, pointer).as_pointer()
}

#[inline(always)]
pub fn equals(
    state: &State,
    compare: Pointer,
    compare_with: Pointer,
) -> Pointer {
    if compare == compare_with {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}

#[inline(always)]
pub fn static_call(
    state: &State,
    generator: &mut GeneratorPointer,
    receiver: Pointer,
    method_index: u16,
    start_reg: u16,
    num_args: u16,
) {
    let index = unsafe { MethodIndex::new(method_index) };
    let method = Method::lookup(&state.permanent_space, receiver, index);
    let mut new_context = ExecutionContext::for_method(method);

    new_context
        .set_registers(generator.context.get_registers(start_reg, num_args));
    generator.push_context(new_context);
}

#[inline(always)]
pub fn dynamic_call(
    state: &State,
    generator: &mut GeneratorPointer,
    receiver: Pointer,
    hash1: u16,
    hash2: u16,
    start_reg: u16,
    num_args: u16,
) {
    let hash = u32_from_u16_pair(hash1, hash2);
    let method = Method::hashed_lookup(&state.permanent_space, receiver, hash);
    let mut new_context = ExecutionContext::for_method(method);

    new_context
        .set_registers(generator.context.get_registers(start_reg, num_args));
    generator.push_context(new_context);
}

#[inline(always)]
pub fn get_method(state: &State, rec_ptr: Pointer, index: u16) -> Pointer {
    let class = Class::of(&state.permanent_space, rec_ptr);

    class
        .get_method(unsafe { MethodIndex::new(index) })
        .as_pointer()
}
