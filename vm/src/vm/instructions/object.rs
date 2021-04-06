//! VM functions for working with Inko objects.
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

#[inline(always)]
pub fn allocate(
    process: &RcProcess,
    proto_ptr: ObjectPointer,
) -> ObjectPointer {
    process.allocate(object_value::none(), proto_ptr)
}

#[inline(always)]
pub fn allocate_permanent(
    state: &RcState,
    proto_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let proto_to_use = if proto_ptr.is_permanent() {
        proto_ptr
    } else {
        state.permanent_allocator.lock().copy_object(proto_ptr)?
    };

    let new_ptr = state
        .permanent_allocator
        .lock()
        .allocate_with_prototype(object_value::none(), proto_to_use);

    Ok(new_ptr)
}

/// Returns a prototype for the given numeric ID.
///
/// This method operates on an i64 instead of some sort of enum, as enums
/// can not be represented in Inko code.
#[inline(always)]
pub fn get_builtin_prototype(
    state: &RcState,
    id: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let id_int = id.integer_value()?;
    let proto = match id_int {
        0 => state.integer_prototype,
        1 => state.float_prototype,
        2 => state.string_prototype,
        3 => state.array_prototype,
        4 => state.block_prototype,
        5 => state.boolean_prototype,
        6 => state.byte_array_prototype,
        7 => state.nil_prototype,
        8 => state.module_prototype,
        9 => state.ffi_library_prototype,
        10 => state.ffi_function_prototype,
        11 => state.ffi_pointer_prototype,
        12 => state.ip_socket_prototype,
        13 => state.unix_socket_prototype,
        14 => state.process_prototype,
        15 => state.read_only_file_prototype,
        16 => state.write_only_file_prototype,
        17 => state.read_write_file_prototype,
        18 => state.hasher_prototype,
        19 => state.generator_prototype,
        20 => state.trait_prototype,
        21 => state.child_process_prototype,
        22 => state.future_prototype,
        _ => return Err(format!("Invalid prototype identifier: {}", id_int)),
    };

    Ok(proto)
}

#[inline(always)]
pub fn get_attribute(
    state: &RcState,
    rec_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    let name = state.intern_pointer(name_ptr).unwrap_or(name_ptr);

    rec_ptr
        .lookup_attribute(&state, name)
        .unwrap_or(state.nil_object)
}

#[inline(always)]
pub fn get_attribute_in_self(
    state: &RcState,
    rec_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    let name = state.intern_pointer(name_ptr).unwrap_or(name_ptr);

    rec_ptr
        .lookup_attribute_in_self(&state, name)
        .unwrap_or(state.nil_object)
}

#[inline(always)]
pub fn set_attribute(
    state: &RcState,
    process: &RcProcess,
    target_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
    value_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    if target_ptr.is_immutable() {
        return Err(RuntimeError::from(
            "Attributes can't be set for immutable objects",
        ));
    }

    let name = if let Ok(ptr) = state.intern_pointer(name_ptr) {
        ptr
    } else {
        copy_if_permanent!(state.permanent_allocator, name_ptr, target_ptr)
    };

    let value =
        copy_if_permanent!(state.permanent_allocator, value_ptr, target_ptr);

    target_ptr.add_attribute(&process, name, value);

    Ok(value)
}

#[inline(always)]
pub fn get_prototype(state: &RcState, src_ptr: ObjectPointer) -> ObjectPointer {
    src_ptr.prototype(&state).unwrap_or(state.nil_object)
}

#[inline(always)]
pub fn object_equals(
    state: &RcState,
    compare: ObjectPointer,
    compare_with: ObjectPointer,
) -> ObjectPointer {
    if compare == compare_with {
        state.true_object
    } else {
        state.false_object
    }
}

#[inline(always)]
pub fn attribute_exists(
    state: &RcState,
    source_ptr: ObjectPointer,
    name_ptr: ObjectPointer,
) -> ObjectPointer {
    let name = state.intern_pointer(name_ptr).unwrap_or(name_ptr);

    if source_ptr.lookup_attribute(&state, name).is_some() {
        state.true_object
    } else {
        state.false_object
    }
}

#[inline(always)]
pub fn copy_blocks(
    state: &RcState,
    target_ptr: ObjectPointer,
    source_ptr: ObjectPointer,
) -> Result<(), RuntimeError> {
    if target_ptr.is_immutable() || source_ptr.is_immutable() {
        return Ok(());
    }

    let object = target_ptr.get_mut();
    let to_impl = source_ptr.get();

    if let Some(map) = to_impl.attributes_map() {
        for (key, val) in map.iter() {
            if val.block_value().is_err() {
                continue;
            }

            let block =
                copy_if_permanent!(state.permanent_allocator, *val, target_ptr);

            if object.lookup_attribute_in_self(*key).is_none() {
                object.add_attribute(*key, block);
            }
        }
    }

    Ok(())
}

#[inline(always)]
pub fn close(pointer: ObjectPointer) {
    pointer.get_mut().value.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::vm::state::State;

    #[test]
    fn test_get_builtin_prototype() {
        let state = State::with_rc(Config::new(), &[]);

        assert!(
            get_builtin_prototype(&state, ObjectPointer::integer(2)).unwrap()
                == state.string_prototype
        );

        assert!(
            get_builtin_prototype(&state, ObjectPointer::integer(5)).unwrap()
                == state.boolean_prototype
        );

        assert!(
            get_builtin_prototype(&state, ObjectPointer::integer(-1)).is_err()
        );
    }
}
