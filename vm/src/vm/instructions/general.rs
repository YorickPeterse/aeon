//! VM functions for which no better category/module exists.
use crate::indexes::{GlobalIndex, LiteralIndex, LocalIndex};
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Header, Int};
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

#[inline(always)]
pub fn get_literal(generator: GeneratorPointer, index: u16) -> Pointer {
    generator
        .context
        .module()
        .get_literal(unsafe { LiteralIndex::from_u16(index) })
}

#[inline(always)]
pub fn get_literal_wide(
    generator: GeneratorPointer,
    a: u16,
    b: u16,
) -> Pointer {
    generator
        .context
        .module()
        .get_literal(unsafe { LiteralIndex::wide(a, b) })
}

#[inline(always)]
pub fn set_local(mut generator: GeneratorPointer, index: u16, value: Pointer) {
    generator
        .context
        .set_local(unsafe { LocalIndex::new(index) }, value);
}

#[inline(always)]
pub fn get_local(generator: GeneratorPointer, index: u16) -> Pointer {
    generator
        .context
        .get_local(unsafe { LocalIndex::new(index) })
}

#[inline(always)]
pub fn set_global(mut generator: GeneratorPointer, index: u16, value: Pointer) {
    unsafe {
        generator.context.set_global(GlobalIndex::new(index), value);
    }
}

#[inline(always)]
pub fn get_global(generator: GeneratorPointer, index: u16) -> Pointer {
    generator
        .context
        .get_global(unsafe { GlobalIndex::new(index) })
}

#[inline(always)]
pub fn exit(state: &State, status_ptr: Pointer) -> Result<(), String> {
    let status = unsafe { Int::read(status_ptr) as i32 };

    state.terminate(status);
    Ok(())
}

#[inline(always)]
pub fn move_result(
    generator: &mut GeneratorPointer,
) -> Result<Pointer, String> {
    generator.take_result().ok_or_else(|| {
        "The last instruction didn't produce a result".to_string()
    })
}

#[inline(always)]
pub fn increment_ref(ptr: Pointer) {
    if !ptr.is_regular_object() {
        return;
    }

    unsafe { ptr.get_mut::<Header>().references += 1 };
}

#[inline(always)]
pub fn decrement_ref(ptr: Pointer) {
    if !ptr.is_regular_object() {
        return;
    }

    unsafe { ptr.get_mut::<Header>().references -= 1 };
}

#[inline(always)]
pub fn drop(ptr: Pointer) -> Result<(), RuntimeError> {
    if !ptr.is_regular_object() {
        return Ok(());
    }

    let header = unsafe { ptr.get_mut::<Header>() };

    if header.references > 0 {
        return Err(RuntimeError::Panic(format!(
            "Could not drop an instance of type {:?}, as it still has {} references",
            header.class.name(),
            header.references
        )));
    }

    unsafe { ptr.recycle(header.class.instance_size()) };
    Ok(())
}
