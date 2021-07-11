//! Functions for working with Inko floats.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Float, Int, String as InkoString, UnsignedInt};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Rounds a float up.
///
/// This function requires a single argument: the float to round.
pub fn float_ceil(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let float = unsafe { Float::read(arguments[0]) };
    let value = float.ceil();

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        value,
    ))
}

/// Rounds a float down.
///
/// This function requires a single argument: the float to round.
pub fn float_floor(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let float = unsafe { Float::read(arguments[0]) };
    let value = float.floor();

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        value,
    ))
}

/// Rounds a float to a given number of decimals.
///
/// This function requires the following arguments:
///
/// 1. The float to round.
/// 2. The number of decimals to round to.
pub fn float_round(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let float = unsafe { Float::read(arguments[0]) };
    let precision = unsafe { UnsignedInt::read(arguments[1]) };
    let result = if precision == 0 {
        float.round()
    } else if precision <= u64::from(u32::MAX) {
        let power = 10.0_f64.powi(precision as i32);
        let multiplied = float * power;

        // Certain very large numbers (e.g. f64::MAX) would produce Infinity
        // when multiplied with the power. In this case we just return the input
        // float directly.
        if multiplied.is_finite() {
            multiplied.round() / power
        } else {
            float
        }
    } else {
        float
    };

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        result,
    ))
}

/// Returns the bitwise representation of the float.
///
/// This function requires a single argument: the float to convert.
pub fn float_to_bits(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bits = unsafe { Float::read(arguments[0]) }.to_bits();

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        bits,
    ))
}

/// Converts a Float to an Integer.
///
/// This function requires a single argument: the float to convert.
pub fn float_to_integer(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let float = unsafe { Float::read(arguments[0]) };

    Ok(Int::alloc(
        alloc,
        state.permanent_space.int_class(),
        float as i64,
    ))
}

/// Converts a Float to a String.
///
/// This function requires a single argument: the float to convert.
pub fn float_to_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = format!("{:?}", unsafe { Float::read(arguments[0]) });

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        value,
    ))
}

/// Returns true if a Float is a NaN.
///
/// This function requires a single argument: the float to check.
pub fn float_is_nan(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { Float::read(arguments[0]) }.is_nan() {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Returns true if a Float is an infinite number.
///
/// This function requires a single argument: the float to check.
pub fn float_is_infinite(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { Float::read(arguments[0]) }.is_infinite() {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Copies a Float
///
/// This function requires a single argument: the Float to copy.
pub fn float_clone(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let src = arguments[0];

    if src.is_permanent() {
        return Ok(src);
    }

    let value = unsafe { Float::read(src) };

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        value,
    ))
}

register!(
    float_ceil,
    float_floor,
    float_round,
    float_to_bits,
    float_to_integer,
    float_to_string,
    float_is_nan,
    float_is_infinite,
    float_clone
);
