//! VM functions for working with Inko strings.
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Int, String as InkoString};
use crate::scheduler::process_worker::ProcessWorker;
use crate::slicing;
use crate::vm::state::State;

#[inline(always)]
pub fn equals(state: &State, left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    if left_ptr.is_permanent() && right_ptr.is_permanent() {
        if left_ptr == right_ptr {
            return state.permanent_space.true_singleton;
        } else {
            return state.permanent_space.false_singleton;
        }
    }

    let left = unsafe { InkoString::read(&left_ptr) };
    let right = unsafe { InkoString::read(&right_ptr) };

    if left == right {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}

#[inline(always)]
pub fn length(
    state: &State,
    worker: &mut ProcessWorker,
    ptr: Pointer,
) -> Pointer {
    let string = unsafe { InkoString::read(&ptr) };
    let length = string.chars().count() as i64;

    Int::alloc(
        worker.allocator(),
        state.permanent_space.int_class(),
        length,
    )
}

#[inline(always)]
pub fn size(
    state: &State,
    worker: &mut ProcessWorker,
    ptr: Pointer,
) -> Pointer {
    let string = unsafe { InkoString::read(&ptr) };
    let length = string.len() as i64;

    Int::alloc(
        worker.allocator(),
        state.permanent_space.int_class(),
        length,
    )
}

#[inline(always)]
pub fn concat(
    state: &State,
    worker: &mut ProcessWorker,
    generator: GeneratorPointer,
    start_reg: u16,
    amount: u16,
) -> Pointer {
    let mut buffer = String::new();

    for register in start_reg..(start_reg + amount) {
        let ptr = generator.context.get_register(register);

        buffer.push_str(unsafe { InkoString::read(&ptr) });
    }

    InkoString::alloc(
        worker.allocator(),
        state.permanent_space.string_class(),
        buffer,
    )
}

#[inline(always)]
pub fn byte(str_ptr: Pointer, index_ptr: Pointer) -> Pointer {
    let string = unsafe { InkoString::read(&str_ptr) };
    let index = slicing::slice_index_to_usize(index_ptr, string.len());
    let byte = i64::from(string.as_bytes()[index]);

    Pointer::int(byte)
}
