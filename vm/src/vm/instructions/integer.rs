// VM functions for working with Inko integers.
use crate::mem::allocator::Pointer;
use crate::mem::objects::{Int, UnsignedInt};
use crate::numeric::{FlooredDiv, Modulo};
use crate::runtime_error::RuntimeError;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::state::State;
use std::ops::{BitAnd, BitOr, BitXor};

/// Generates an expression for a binary operation that produces a new integer.
macro_rules! int_op {
    ($kind: ident, $left: expr, $right: expr, $op: ident) => {{
        let left = unsafe { $kind::read($left) };
        let right = unsafe { $kind::read($right) };

        left.$op(right)
    }};
}

/// Generates an expression for a binary shift.
macro_rules! int_shift {
    ($kind: ident, $left: expr, $right: expr, $op: ident) => {{
        let left = unsafe { $kind::read($left) };
        let right = unsafe { $kind::read($right) };

        left.$op(right as u32)
    }};
}

/// Generates an expression for a binary operation that produces a boolean.
macro_rules! int_bool {
    ($kind: ident, $state: expr, $left: expr, $right: expr, $op: tt) => {{
        let left = unsafe { $kind::read($left) };
        let right = unsafe { $kind::read($right) };

        if left $op right {
            $state.permanent_space.true_singleton
        } else {
            $state.permanent_space.false_singleton
        }
    }};
}

/// Generates an expression for integer division.
macro_rules! int_div {
    ($kind: ident, $left: expr, $right: expr) => {{
        let left = unsafe { $kind::read($left) };
        let right = unsafe { $kind::read($right) };

        if right == 0 {
            return Err(RuntimeError::Panic(
                "Integer types can't be divided by zero".to_string(),
            ));
        }

        left.floored_division(right)
    }};
}

#[inline(always)]
pub fn int_add(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(Int, left, right, wrapping_add);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_div(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, RuntimeError> {
    let value = int_div!(Int, left, right);
    let result = Int::alloc(
        worker.allocator(),
        state.permanent_space.int_class(),
        value,
    );

    Ok(result)
}

#[inline(always)]
pub fn int_mul(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(Int, left, right, wrapping_mul);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_sub(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(Int, left, right, wrapping_sub);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_modulo(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(Int, left, right, modulo);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_and(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(Int, left, right, bitand);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_or(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(Int, left, right, bitor);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_xor(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(Int, left, right, bitxor);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_shl(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_shift!(Int, left, right, wrapping_shl);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_shr(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_shift!(Int, left, right, wrapping_shr);

    Int::alloc(worker.allocator(), state.permanent_space.int_class(), value)
}

#[inline(always)]
pub fn int_smaller(state: &State, left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, state, left, right, <)
}

#[inline(always)]
pub fn int_greater(state: &State, left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, state, left, right, >)
}

#[inline(always)]
pub fn int_equals(state: &State, left: Pointer, right: Pointer) -> Pointer {
    int_bool!(Int, state, left, right, ==)
}

#[inline(always)]
pub fn int_greater_or_equal(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    int_bool!(Int, state, left, right, >=)
}

#[inline(always)]
pub fn int_smaller_or_equal(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    int_bool!(Int, state, left, right, <=)
}

#[inline(always)]
pub fn unsigned_int_add(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(UnsignedInt, left, right, wrapping_add);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_div(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Result<Pointer, RuntimeError> {
    let value = int_div!(UnsignedInt, left, right);
    let result = UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    );

    Ok(result)
}

#[inline(always)]
pub fn unsigned_int_mul(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(UnsignedInt, left, right, wrapping_mul);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_sub(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(UnsignedInt, left, right, wrapping_sub);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_modulo(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(UnsignedInt, left, right, modulo);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_and(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(UnsignedInt, left, right, bitand);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_or(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(UnsignedInt, left, right, bitor);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_xor(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_op!(UnsignedInt, left, right, bitxor);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_shl(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_shift!(UnsignedInt, left, right, wrapping_shl);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_shr(
    state: &State,
    worker: &mut ProcessWorker,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    let value = int_shift!(UnsignedInt, left, right, wrapping_shr);

    UnsignedInt::alloc(
        worker.allocator(),
        state.permanent_space.unsigned_int_class(),
        value,
    )
}

#[inline(always)]
pub fn unsigned_int_smaller(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    int_bool!(UnsignedInt, state, left, right, <)
}

#[inline(always)]
pub fn unsigned_int_greater(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    int_bool!(UnsignedInt, state, left, right, >)
}

#[inline(always)]
pub fn unsigned_int_equals(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    int_bool!(UnsignedInt, state, left, right, ==)
}

#[inline(always)]
pub fn unsigned_int_greater_or_equal(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    int_bool!(UnsignedInt, state, left, right, >=)
}

#[inline(always)]
pub fn unsigned_int_smaller_or_equal(
    state: &State,
    left: Pointer,
    right: Pointer,
) -> Pointer {
    int_bool!(UnsignedInt, state, left, right, <=)
}
