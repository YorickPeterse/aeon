//! Functions for generating random numbers.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{ByteArray, Float, Int, UnsignedInt};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;
use rand::{thread_rng, Rng};
use std::cell::Cell;

macro_rules! verify_min_max {
    ($min:expr, $max:expr) => {
        if $min >= $max {
            return Err(format!(
                "The lower bound {} must be lower than the upper bound {}",
                $min, $max
            )
            .into());
        }
    };
}

thread_local! {
    /// A randomly generated, thread-local integer that can be incremented.
    ///
    /// This is useful when generating seed keys for hash maps. The first time
    /// this is used, a random number is generated. After that, the number is
    /// simply incremented (and wraps upon overflowing). This removes the need
    /// for generating a random number every time, which can be expensive.
    static INCREMENTAL_INTEGER: Cell<u64> = Cell::new(thread_rng().gen());
}

/// Generates a random unsigned integer.
///
/// This function doesn't take any arguments.
pub fn random_integer(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = thread_rng().gen();

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value,
    ))
}

/// Generates an unsigned integer that starts of with a random value, then is
/// incremented on every call.
///
/// This function doesn't take any arguments.
pub fn random_incremental_integer(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value =
        INCREMENTAL_INTEGER.with(|num| num.replace(num.get().wrapping_add(1)));

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value,
    ))
}

/// Generates a random float.
///
/// This function doesn't take any arguments.
pub fn random_float(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = thread_rng().gen();

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        value,
    ))
}

/// Generates a random signed integer in a range.
///
/// This function takes two arguments:
///
/// 1. The lower bound of the range.
/// 2. The upper bound of the range.
pub fn random_integer_range(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let min = unsafe { Int::read(arguments[0]) };
    let max = unsafe { Int::read(arguments[1]) };
    let mut rng = thread_rng();

    verify_min_max!(min, max);

    Ok(Int::alloc(
        alloc,
        state.permanent_space.int_class(),
        rng.gen_range(min, max),
    ))
}

/// Generates a random float in a range.
///
/// This function takes two arguments:
///
/// 1. The lower bound of the range.
/// 2. The upper bound of the range.
pub fn random_float_range(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let min = unsafe { Float::read(arguments[0]) };
    let max = unsafe { Float::read(arguments[1]) };
    let mut rng = thread_rng();

    verify_min_max!(min, max);

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        rng.gen_range(min, max),
    ))
}

/// Generates a random sequence of bytes.
///
/// This function takes a single argument: the number of bytes to generate.
pub fn random_bytes(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let size = unsafe { UnsignedInt::read(arguments[0]) } as usize;
    let mut bytes = Vec::with_capacity(size);

    unsafe {
        bytes.set_len(size);
    }

    thread_rng()
        .try_fill(&mut bytes[..])
        .map_err(|e| e.to_string())?;

    Ok(ByteArray::alloc(
        alloc,
        state.permanent_space.byte_array_class(),
        bytes,
    ))
}

register!(
    random_integer,
    random_incremental_integer,
    random_float,
    random_integer_range,
    random_float_range,
    random_bytes
);
