//! Functions for working with Inko strings.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{
    Array, ByteArray, Float, Int, String as InkoString, UnsignedInt,
};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::slicing;
use crate::vm::state::State;

macro_rules! verify_radix {
    ($radix: expr) => {{
        if !(2..=36).contains(&$radix) {
            return Err(RuntimeError::Panic(format!(
                "radix must be between 2 and 32, not {}",
                $radix
            )));
        }
    }};
}

/// Converts a String to lowercase.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_lower(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let lower = unsafe { InkoString::read(&arguments[0]).to_lowercase() };

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        lower,
    ))
}

/// Converts a String to lowercase.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_upper(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let upper = unsafe { InkoString::read(&arguments[0]).to_uppercase() };

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        upper,
    ))
}

/// Converts a String to a ByteArray.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_byte_array(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes = unsafe { InkoString::read(&arguments[0]).as_bytes().to_vec() };

    Ok(ByteArray::alloc(
        alloc,
        state.permanent_space.byte_array_class(),
        bytes,
    ))
}

/// Concatenates multiple Strings together.
///
/// This function requires a single argument: an array of strings to
/// concatenate.
pub fn string_concat_array(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let array = unsafe { arguments[0].get::<Array>() }.value();
    let mut buffer = String::new();

    for str_ptr in array.iter() {
        buffer.push_str(unsafe { InkoString::read(&*str_ptr) });
    }

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        buffer,
    ))
}

/// Formats a String for debugging purposes.
///
/// This function requires a single argument: the string to format.
pub fn string_format_debug(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = format!("{:?}", unsafe { InkoString::read(&arguments[0]) });

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        value,
    ))
}

/// Slices a String into a new String.
///
/// This function requires the following arguments:
///
/// 1. The String to slice.
/// 2. The start position of the slice.
/// 3. The number of characters to include in the new slice.
pub fn string_slice(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&arguments[0]) };
    let start =
        slicing::slice_index_to_usize(arguments[1], string.chars().count());

    let amount = unsafe { UnsignedInt::read(arguments[2]) } as usize;
    let new_string =
        string.chars().skip(start).take(amount).collect::<String>();

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        new_string,
    ))
}

/// Converts a String to an integer.
///
/// This function requires the following arguments:
///
/// 1. The String to convert.
/// 2. The radix to use for converting the String to an Integer.
pub fn string_to_int(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&arguments[0]) };
    let radix = unsafe { UnsignedInt::read(arguments[1]) };

    verify_radix!(radix);

    i64::from_str_radix(string, radix as u32)
        .map(|val| Int::alloc(alloc, state.permanent_space.int_class(), val))
        .map_err(|_| {
            RuntimeError::Error(state.permanent_space.undefined_singleton)
        })
}

/// Converts a String to an unsigned integer.
///
/// This function requires the following arguments:
///
/// 1. The String to convert.
/// 2. The radix to use for converting the String to an Integer.
pub fn string_to_unsigned_int(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&arguments[0]) };
    let radix = unsafe { Int::read(arguments[1]) };

    verify_radix!(radix);

    u64::from_str_radix(string, radix as u32)
        .map(|val| {
            UnsignedInt::alloc(
                alloc,
                state.permanent_space.unsigned_int_class(),
                val,
            )
        })
        .map_err(|_| {
            RuntimeError::Error(state.permanent_space.undefined_singleton)
        })
}

/// Converts a String to a Float.
///
/// This function requires a single argument: the string to convert.
pub fn string_to_float(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let string = unsafe { InkoString::read(&arguments[0]) };

    string
        .parse::<f64>()
        .map(|val| {
            Float::alloc(alloc, state.permanent_space.float_class(), val)
        })
        .map_err(|_| {
            RuntimeError::Error(state.permanent_space.undefined_singleton)
        })
}

/// Copies a String.
///
/// This function requires a single argument: the String to copy.
pub fn string_clone(
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

    let val = unsafe { src.get::<InkoString>().value().clone() };

    Ok(InkoString::from_immutable_string(
        alloc,
        state.permanent_space.string_class(),
        val,
    ))
}

/// Drops a String.
///
/// This function requires a single argument: the String to drop.
pub fn string_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = arguments[0];

    // Permanent Strings can be used as if they were regular owned Strings, so
    // we must make sure not to drop these.
    if !value.is_permanent() {
        unsafe {
            InkoString::drop(value);
        }
    }

    Ok(state.permanent_space.nil_singleton)
}

register!(
    string_to_lower,
    string_to_upper,
    string_to_byte_array,
    string_concat_array,
    string_format_debug,
    string_slice,
    string_to_int,
    string_to_unsigned_int,
    string_to_float,
    string_clone,
    string_drop
);
