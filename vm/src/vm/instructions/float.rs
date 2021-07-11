//! VM functions for working with Inko floats.
use crate::mem::allocator::Pointer;
use crate::mem::objects::Float;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::state::State;
use float_cmp::ApproxEqUlps;

#[inline(always)]
pub fn add(
    state: &State,
    worker: &mut ProcessWorker,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left + right;

    Float::alloc(
        worker.allocator(),
        state.permanent_space.float_class(),
        value,
    )
}

#[inline(always)]
pub fn mul(
    state: &State,
    worker: &mut ProcessWorker,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left * right;

    Float::alloc(
        worker.allocator(),
        state.permanent_space.float_class(),
        value,
    )
}

#[inline(always)]
pub fn div(
    state: &State,
    worker: &mut ProcessWorker,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left / right;

    Float::alloc(
        worker.allocator(),
        state.permanent_space.float_class(),
        value,
    )
}

#[inline(always)]
pub fn sub(
    state: &State,
    worker: &mut ProcessWorker,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left - right;

    Float::alloc(
        worker.allocator(),
        state.permanent_space.float_class(),
        value,
    )
}

#[inline(always)]
pub fn modulo(
    state: &State,
    worker: &mut ProcessWorker,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };
    let value = left % right;

    Float::alloc(
        worker.allocator(),
        state.permanent_space.float_class(),
        value,
    )
}

#[inline(always)]
pub fn equals(state: &State, left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if !left.is_nan() && !right.is_nan() && left.approx_eq_ulps(&right, 1) {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}

#[inline(always)]
pub fn smaller(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left < right {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}

#[inline(always)]
pub fn greater(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left > right {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}

#[inline(always)]
pub fn greater_or_equal(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left >= right {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}

#[inline(always)]
pub fn smaller_or_equal(
    state: &State,
    left_ptr: Pointer,
    right_ptr: Pointer,
) -> Pointer {
    let left = unsafe { Float::read(left_ptr) };
    let right = unsafe { Float::read(right_ptr) };

    if left <= right {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    }
}
