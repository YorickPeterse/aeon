//! FFI for interfacing with C code.
//!
//! This module provides types and methods for interfacing with C code, using
//! libffi.
use crate::mem::allocator::{BumpAllocator, Pointer as InkoPointer};
use crate::mem::objects::{
    ByteArray, Class, Float, Int, String as InkoString, UnsignedInt,
};
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;
use libffi::low::{
    call as ffi_call, ffi_abi_FFI_DEFAULT_ABI as ABI, ffi_cif, ffi_type,
    prep_cif, types, CodePtr, Error as FFIError,
};
use std::convert::Into;
use std::ffi::{CStr, OsStr};
use std::fmt::{Debug, Display};
use std::mem;
use std::os::raw::{
    c_char, c_double, c_float, c_int, c_long, c_short, c_uchar, c_uint,
    c_ulong, c_ushort, c_void,
};
use std::ptr;

/// Returns a pointer to a statically allocated FFI type.
macro_rules! ffi_type {
    ($name: ident) => {
        &types::$name as *const ffi_type as *mut ffi_type
    };
}

/// Converts a &T to a *mut c_void pointer.
macro_rules! raw_pointer {
    ($value: expr) => {
        $value as *mut _ as RawPointer
    };
}

/// Generates a "match" that can be used for pattern matching a pointer to an
/// FFI type.
///
/// For example, this macro call:
///
///     match_ffi_type!(
///       some_variable,
///       pointer => { 10 }
///       void => { 20 }
///     );
///
/// Would compile into:
///
///     match some_variable {
///         t if t == ffi_type!(pointer) => { 10 }
///         t if t == ffi_type!(void) => { 20 }
///         _ => unreachable!()
///     }
///
/// Just like a regular `match`, `match_ffi_type!` supports OR conditions:
///
///     match_ffi_type!(
///       some_variable,
///       pointer => { 10 }
///       void => { 20 }
///       sint8 | sint16 | sint32 | sint64 => { 30 }
///     );
///
/// This would compile into the following:
///
///     match some_variable {
///         t if t == ffi_type!(pointer) => { 10 }
///         t if t == ffi_type!(void) => { 20 }
///         t if t == ffi_type!(sint8) => { 30 }
///         t if t == ffi_type!(sint16) => { 30 }
///         t if t == ffi_type!(sint32) => { 30 }
///         t if t == ffi_type!(sint64) => { 30 }
///         _ => unreachable!()
///     }
macro_rules! match_ffi_type {
    (
        $pointer: expr,

        $(
            $($type: ident)|+ => $body: expr
        )+
    ) => {
        match $pointer {
            $(
                $(
                    t if t == ffi_type!($type) => { $body }
                )+
            )+
            _ => unreachable!()
        }
    }
}

macro_rules! ffi_type_error {
    ($type: expr) => {
        return Err(format!("Invalid FFI type: {}", $type));
    };
}

/// The numeric identifier of the C `void` type.
const TYPE_VOID: u64 = 0;

/// The numeric identifier of the C `void*` type.
const TYPE_POINTER: u64 = 1;

/// The numeric identifier of the C `double` type.
const TYPE_DOUBLE: u64 = 2;

/// The numeric identifier of the C `float` type.
const TYPE_FLOAT: u64 = 3;

/// The numeric identifier of the C `signed char` type.
const TYPE_I8: u64 = 4;

/// The numeric identifier of the C `short` type.
const TYPE_I16: u64 = 5;

/// The numeric identifier of the C `int` type.
const TYPE_I32: u64 = 6;

/// The numeric identifier of the C `long` type.
const TYPE_I64: u64 = 7;

/// The numeric identifier of the C `unsigned char` type.
const TYPE_U8: u64 = 8;

/// The numeric identifier of the C `unsigned short` type.
const TYPE_U16: u64 = 9;

/// The numeric identifier of the C `unsigned int` type.
const TYPE_U32: u64 = 10;

/// The numeric identifier of the C `unsigned long` type.
const TYPE_U64: u64 = 11;

/// The numeric identifier for the C `const char*` type.
const TYPE_STRING: u64 = 12;

/// The numeric identifier for a C `const char*` type that should be read into a
/// byte array..
const TYPE_BYTE_ARRAY: u64 = 13;

/// The numeric identifier of the C `size_t` type.
const TYPE_SIZE_T: u64 = 14;

/// A C library, such as libc.
///
/// This is currently a thin wrapper around libloading's Library structure,
/// allowing us to decouple the rest of the VM code from libloading.
pub struct Library {
    inner: libloading::Library,
}

/// A pointer to an FFI type.
pub type TypePointer = *mut ffi_type;

/// A raw C pointer.
pub type RawPointer = *mut c_void;

/// A wrapper around a C pointer.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Pointer {
    inner: RawPointer,
}

unsafe impl Send for Pointer {}

/// A function with a fixed number of arguments.
pub struct Function {
    /// The pointer to the function to call.
    pointer: Pointer,

    /// The CIF (Call Interface) to use for this function.
    cif: ffi_cif,

    /// The argument types of the function.
    arguments: Vec<TypePointer>,

    /// The return type of the function.
    return_type: TypePointer,
}

/// Returns the size of a type ID.
///
/// The size of the type is returned as a tagged integer.
pub fn type_size(id: u64) -> Result<InkoPointer, String> {
    let size = unsafe {
        match id {
            TYPE_VOID => types::void.size,
            TYPE_POINTER | TYPE_STRING | TYPE_BYTE_ARRAY => types::pointer.size,
            TYPE_DOUBLE => types::double.size,
            TYPE_FLOAT => types::float.size,
            TYPE_I8 => types::sint8.size,
            TYPE_I16 => types::sint16.size,
            TYPE_I32 => types::sint32.size,
            TYPE_I64 => types::sint64.size,
            TYPE_U8 => types::uint8.size,
            TYPE_U16 => types::uint16.size,
            TYPE_U32 => types::uint32.size,
            TYPE_U64 => types::uint64.size,
            TYPE_SIZE_T => mem::size_of::<usize>(),
            _ => ffi_type_error!(id),
        }
    };

    Ok(InkoPointer::int(size as i64))
}

/// Returns the alignment of a type ID.
///
/// The alignment of the type is returned as a tagged integer.
pub fn type_alignment(id: u64) -> Result<InkoPointer, String> {
    let size = unsafe {
        match id {
            TYPE_VOID => types::void.alignment,
            TYPE_POINTER | TYPE_STRING | TYPE_BYTE_ARRAY => {
                types::pointer.alignment
            }
            TYPE_DOUBLE => types::double.alignment,
            TYPE_FLOAT => types::float.alignment,
            TYPE_I8 => types::sint8.alignment,
            TYPE_I16 => types::sint16.alignment,
            TYPE_I32 => types::sint32.alignment,
            TYPE_I64 => types::sint64.alignment,
            TYPE_U8 => types::uint8.alignment,
            TYPE_U16 => types::uint16.alignment,
            TYPE_U32 => types::uint32.alignment,
            TYPE_U64 => types::uint64.alignment,
            TYPE_SIZE_T => mem::align_of::<usize>() as u16,
            _ => ffi_type_error!(id),
        }
    };

    Ok(InkoPointer::int(i64::from(size)))
}

/// A value of some sort to be passed to a C function.
pub enum Argument {
    Pointer(RawPointer),
    Void,
    F32(f32),
    F64(f64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
}

impl Argument {
    // Creates a new Argument wrapping the value of `ptr` according to the needs
    // of the FFI type specified in `ffi_type`.
    unsafe fn wrap(
        state: &State,
        ffi_type: *mut ffi_type,
        ptr: InkoPointer,
    ) -> Result<Argument, String> {
        let argument = match_ffi_type!(
            ffi_type,
            pointer => {
                let class = Class::of(&state.permanent_space, ptr);
                let value = if class == state.permanent_space.string_class() {
                    ptr.get::<InkoString>().value().as_c_char_pointer() as RawPointer
                } else if class == state.permanent_space.byte_array_class(){
                    ptr.get::<ByteArray>().value().as_ptr() as RawPointer
                } else {
                    ptr.as_ptr() as RawPointer
                };

                Argument::Pointer(value)
            }
            void => Argument::Void
            float => Argument::F32(Float::read(ptr) as f32)
            double => Argument::F64(Float::read(ptr))
            sint8 => Argument::I8(Int::read(ptr) as i8)
            sint16 => Argument::I16(Int::read(ptr) as i16)
            sint32 => Argument::I32(Int::read(ptr) as i32)
            sint64 => Argument::I64(Int::read(ptr) as i64)
            uint8 => Argument::U8(Int::read(ptr) as u8)
            uint16 => Argument::U16(Int::read(ptr) as u16)
            uint32 => Argument::U32(Int::read(ptr) as u32)
            uint64 => Argument::U64(Int::read(ptr) as u64)
        );

        Ok(argument)
    }

    /// Returns a C pointer to the wrapped value.
    fn as_c_pointer(&mut self) -> RawPointer {
        match self {
            Argument::Pointer(ref mut val) => {
                // When passing a pointer we shouldn't pass the pointer
                // directly, instead we want a pointer to the pointer to pass to
                // the underlying C function.
                val as *mut RawPointer as RawPointer
            }
            Argument::Void => ptr::null_mut() as RawPointer,
            Argument::F32(ref mut val) => raw_pointer!(val),
            Argument::F64(ref mut val) => raw_pointer!(val),
            Argument::I8(ref mut val) => raw_pointer!(val),
            Argument::I16(ref mut val) => raw_pointer!(val),
            Argument::I32(ref mut val) => raw_pointer!(val),
            Argument::I64(ref mut val) => raw_pointer!(val),
            Argument::U8(ref mut val) => raw_pointer!(val),
            Argument::U16(ref mut val) => raw_pointer!(val),
            Argument::U32(ref mut val) => raw_pointer!(val),
            Argument::U64(ref mut val) => raw_pointer!(val),
        }
    }
}

/// Returns an FFI type for an integer pointer.
unsafe fn ffi_type_for(pointer: InkoPointer) -> Result<TypePointer, String> {
    let int = UnsignedInt::read(pointer);
    let typ = match int {
        TYPE_VOID => ffi_type!(void),
        TYPE_POINTER | TYPE_STRING | TYPE_BYTE_ARRAY => ffi_type!(pointer),
        TYPE_DOUBLE => ffi_type!(double),
        TYPE_FLOAT => ffi_type!(float),
        TYPE_I8 => ffi_type!(sint8),
        TYPE_I16 => ffi_type!(sint16),
        TYPE_I32 => ffi_type!(sint32),
        TYPE_I64 => ffi_type!(sint64),
        TYPE_U8 => ffi_type!(uint8),
        TYPE_U16 => ffi_type!(uint16),
        TYPE_U32 => ffi_type!(uint32),
        TYPE_U64 => ffi_type!(uint64),
        TYPE_SIZE_T => {
            match mem::size_of::<usize>() {
                64 => ffi_type!(uint64),
                32 => ffi_type!(uint32),
                8 => ffi_type!(uint8),

                // The C spec states that `size_t` is at least 16 bits, so we
                // can use this as the default.
                _ => ffi_type!(uint16),
            }
        }
        _ => ffi_type_error!(int),
    };

    Ok(typ as TypePointer)
}

impl Library {
    /// Opens a library using one or more possible names, stored as pointers to
    /// heap allocated objects.
    pub fn from_pointers(
        search_for: &[InkoPointer],
    ) -> Result<Library, String> {
        let mut names = Vec::with_capacity(search_for.len());

        for name in search_for {
            names.push(unsafe { InkoString::read(&*name) });
        }

        Self::open(&names)
    }

    /// Opens a library using one or more possible names.
    pub fn open<P: AsRef<OsStr> + Debug + Display>(
        search_for: &[P],
    ) -> Result<Library, String> {
        let mut errors = Vec::new();

        for name in search_for {
            match libloading::Library::new(name).map(|inner| Library { inner })
            {
                Ok(library) => return Ok(library),
                Err(err) => {
                    errors.push(format!("\n{}: {}", name, err));
                }
            }
        }

        let mut error_message =
            "Unable to open the supplied libraries:\n".to_string();

        for error in errors {
            error_message.push_str(&error);
        }

        Err(error_message)
    }

    /// Obtains a pointer to a symbol.
    ///
    /// This method is unsafe because the pointer could be of any type, thus it
    /// is up to the caller to make sure the result is used appropriately.
    pub unsafe fn get(&self, name: &str) -> Result<Pointer, String> {
        self.inner
            .get(name.as_bytes())
            .map(|sym: libloading::Symbol<RawPointer>| Pointer::new(*sym))
            .map_err(|err| err.to_string())
    }
}

impl Pointer {
    pub fn new(inner: RawPointer) -> Self {
        Pointer { inner }
    }

    /// Returns the address of this pointer.
    pub fn address(self) -> usize {
        self.inner as usize
    }

    /// Reads the value of this pointer into a particular type, based on the
    /// integer specified in `kind`.
    pub unsafe fn read_as(
        self,
        state: &State,
        alloc: &mut BumpAllocator,
        kind: InkoPointer,
    ) -> Result<InkoPointer, String> {
        let int = UnsignedInt::read(kind);
        let pointer = match int {
            TYPE_POINTER => {
                let ptr: RawPointer = self.read();

                InkoPointer::new(ptr as *mut u8)
            }
            TYPE_STRING => {
                let string = self.read_cstr().to_string_lossy().into_owned();

                InkoString::alloc(
                    alloc,
                    state.permanent_space.string_class(),
                    string,
                )
            }
            TYPE_BYTE_ARRAY => {
                let bytes = self.read_cstr().to_bytes().to_vec();

                ByteArray::alloc(
                    alloc,
                    state.permanent_space.byte_array_class(),
                    bytes,
                )
            }
            TYPE_DOUBLE => self.read_float::<c_double>(state, alloc),
            TYPE_FLOAT => self.read_float::<c_float>(state, alloc),
            TYPE_I8 => self.read_signed_integer::<c_char>(state, alloc),
            TYPE_I16 => self.read_signed_integer::<c_short>(state, alloc),
            TYPE_I32 => self.read_signed_integer::<c_int>(state, alloc),
            TYPE_I64 => self.read_signed_integer::<c_long>(state, alloc),
            TYPE_U8 => self.read_unsigned_integer::<c_uchar>(state, alloc),
            TYPE_U16 => self.read_unsigned_integer::<c_ushort>(state, alloc),
            TYPE_U32 => self.read_unsigned_integer::<c_uint>(state, alloc),
            TYPE_U64 => self.read_unsigned_integer::<c_ulong>(state, alloc),
            TYPE_SIZE_T => match mem::size_of::<usize>() {
                64 => self.read_unsigned_integer::<c_ulong>(state, alloc),
                32 => self.read_unsigned_integer::<c_uint>(state, alloc),
                16 => self.read_unsigned_integer::<c_ushort>(state, alloc),
                8 => self.read_unsigned_integer::<c_uchar>(state, alloc),
                _ => unreachable!(),
            },
            _ => ffi_type_error!(int),
        };

        Ok(pointer)
    }

    /// Writes a value to the underlying pointer.
    pub unsafe fn write_as(
        self,
        kind: InkoPointer,
        value: InkoPointer,
    ) -> Result<(), String> {
        let int = UnsignedInt::read(kind);

        match int {
            TYPE_STRING => {
                let string = value.get::<InkoString>().value();

                ptr::copy(
                    string.as_c_char_pointer(),
                    self.inner as *mut c_char,
                    string.len_with_null_byte(),
                );
            }
            TYPE_BYTE_ARRAY => {
                let byte_array = value.get::<ByteArray>().value();

                ptr::copy(
                    byte_array.as_ptr(),
                    self.inner as *mut _,
                    byte_array.len(),
                );
            }
            TYPE_POINTER => self.write(value.as_ptr() as RawPointer),
            TYPE_DOUBLE => self.write(Float::read(value)),
            TYPE_FLOAT => self.write(Float::read(value) as f32),
            TYPE_I8 => self.write(Int::read(value) as i8),
            TYPE_I16 => self.write(Int::read(value) as i16),
            TYPE_I32 => self.write(Int::read(value) as i32),
            TYPE_I64 => self.write(Int::read(value) as i64),
            TYPE_U8 => self.write(Int::read(value) as u8),
            TYPE_U16 => self.write(Int::read(value) as u16),
            TYPE_U32 => self.write(Int::read(value) as u32),
            TYPE_U64 => self.write(Int::read(value) as u64),
            TYPE_SIZE_T => self.write(Int::read(value) as usize),
            _ => ffi_type_error!(int),
        };

        Ok(())
    }

    /// Returns a new Pointer, optionally starting at the given offset.
    ///
    /// The `offset` argument is the offset in _bytes_, not the number of
    /// elements (unlike Rust's `pointer::offset`).
    pub fn with_offset(self, offset_bytes: usize) -> Self {
        let inner = (self.inner as usize + offset_bytes) as RawPointer;

        Pointer::new(inner)
    }

    /// Returns the underlying pointer.
    pub fn as_ptr(self) -> *mut u8 {
        self.inner as _
    }

    unsafe fn read<R>(self) -> R {
        ptr::read(self.inner as *mut R)
    }

    unsafe fn write<T>(self, value: T) {
        ptr::write(self.inner as *mut T, value);
    }

    unsafe fn read_signed_integer<T: Into<i64>>(
        self,
        state: &State,
        alloc: &mut BumpAllocator,
    ) -> InkoPointer {
        Int::alloc(
            alloc,
            state.permanent_space.int_class(),
            self.read::<T>().into(),
        )
    }

    unsafe fn read_unsigned_integer<T: Into<u64>>(
        self,
        state: &State,
        alloc: &mut BumpAllocator,
    ) -> InkoPointer {
        Int::alloc(
            alloc,
            state.permanent_space.int_class(),
            self.read::<u64>() as i64,
        )
    }

    unsafe fn read_float<T: Into<f64>>(
        self,
        state: &State,
        alloc: &mut BumpAllocator,
    ) -> InkoPointer {
        Float::alloc(
            alloc,
            state.permanent_space.float_class(),
            self.read::<T>().into(),
        )
    }

    unsafe fn read_cstr<'a>(self) -> &'a CStr {
        CStr::from_ptr(self.inner as *mut c_char)
    }
}

impl Function {
    /// Creates a new function using object pointers.
    pub unsafe fn attach(
        library: &Library,
        name: &str,
        arguments: &[InkoPointer],
        return_type: InkoPointer,
    ) -> Result<Function, RuntimeError> {
        let func_ptr = library.get(name).map_err(RuntimeError::ErrorMessage)?;
        let ffi_rtype = ffi_type_for(return_type)?;
        let mut ffi_arg_types = Vec::with_capacity(arguments.len());

        for ptr in arguments {
            ffi_arg_types.push(ffi_type_for(*ptr)?);
        }

        Self::create(func_ptr, ffi_arg_types, ffi_rtype).map_err(|e| e.into())
    }

    /// Creates a new prepared function.
    unsafe fn create(
        pointer: Pointer,
        arguments: Vec<TypePointer>,
        return_type: TypePointer,
    ) -> Result<Function, String> {
        let mut func = Function {
            pointer,
            cif: Default::default(),
            arguments,
            return_type,
        };

        let result = prep_cif(
            &mut func.cif,
            ABI,
            func.arguments.len(),
            func.return_type,
            func.arguments.as_mut_ptr(),
        );

        match result {
            Ok(_) => Ok(func),
            Err(FFIError::Typedef) => {
                Err("The type representation is invalid or unsupported"
                    .to_string())
            }
            Err(FFIError::Abi) => {
                Err("The ABI is invalid or unsupported".to_string())
            }
        }
    }

    /// Calls the function with the given arguments.
    pub unsafe fn call(
        &self,
        state: &State,
        alloc: &mut BumpAllocator,
        arg_ptrs: &[InkoPointer],
    ) -> Result<InkoPointer, String> {
        if arg_ptrs.len() != self.arguments.len() {
            return Err(format!(
                "Invalid number of arguments, expected {} but got {}",
                self.arguments.len(),
                arg_ptrs.len()
            ));
        }

        let mut arguments = Vec::with_capacity(arg_ptrs.len());

        for (index, arg) in arg_ptrs.iter().enumerate() {
            arguments.push(Argument::wrap(state, self.arguments[index], *arg)?);
        }

        // libffi expects an array of _pointers_ to the arguments to pass,
        // instead of an array containing the arguments directly. The pointers
        // and the values they point to must outlive the FFI call, otherwise we
        // may end up passing pointers to invalid memory.
        let mut argument_pointers: Vec<RawPointer> =
            arguments.iter_mut().map(Argument::as_c_pointer).collect();

        // libffi requires a mutable pointer to the CIF, but "self" is immutable
        // since we never actually modify the current function. To work around
        // this we manually cast to a mutable pointer.
        let cif_ptr = &self.cif as *const _ as *mut _;
        let fun_ptr = CodePtr::from_ptr(self.pointer.inner);
        let args_ptr = argument_pointers.as_mut_ptr();

        // Instead of reading the result into some kind of generic pointer (*mut
        // c_void for example) and trying to cast that to the right type, we'll
        // immediately read the call's return value into the right type. This
        // requires a bit more code, but is much less unsafe than trying to cast
        // types from X to Y without knowing if this even works reliably.
        let pointer = match_ffi_type!(
            self.return_type,
            pointer => {
                InkoPointer::new(ffi_call(cif_ptr, fun_ptr, args_ptr))
            }
            void => {
                ffi_call::<c_void>(cif_ptr, fun_ptr, args_ptr);

                state.permanent_space.nil_singleton
            }
            double | float => {
                let result: c_double = ffi_call(cif_ptr, fun_ptr, args_ptr);

                Float::alloc(
                    alloc,
                    state.permanent_space.float_class(),
                    result as f64
                )
            }
            sint8 | sint16 | sint32 | sint64 => {
                let result: c_long = ffi_call(cif_ptr, fun_ptr, args_ptr);

                Int::alloc(
                    alloc,
                    state.permanent_space.int_class(),
                    result as i64
                )
            }
            uint8 | uint16 | uint32 | uint64 => {
                let result: c_ulong = ffi_call(cif_ptr, fun_ptr, args_ptr);

                Int::alloc(
                    alloc,
                    state.permanent_space.int_class(),
                    result as i64
                )
            }
        );

        Ok(pointer)
    }
}

#[cfg(all(
    test,
    any(target_os = "macos", target_os = "linux", target_os = "windows")
))]
mod tests {
    use super::*;
    use crate::mem::allocator::Pointer as InkoPointer;
    use crate::mem::objects::String as InkoString;
    use crate::test::setup;

    extern "C" {
        fn calloc(amount: usize, size: usize) -> RawPointer;
        fn free(pointer: RawPointer);
    }

    #[cfg(target_os = "macos")]
    const LIBM: &'static str = "libm.dylib";

    #[cfg(target_os = "linux")]
    const LIBM: &'static str = "libm.so.6";

    #[cfg(target_os = "windows")]
    const LIBM: &'static str = "msvcrt.dll";

    #[test]
    fn test_library_new() {
        assert!(Library::open(&[LIBM]).is_ok());
    }

    #[test]
    fn test_library_get() {
        let lib = Library::open(&[LIBM]).unwrap();
        let sym = unsafe { lib.get("floor") };

        assert!(sym.is_ok());
    }

    #[test]
    fn test_function_new() {
        let lib = Library::open(&[LIBM]).unwrap();

        unsafe {
            let sym = lib.get("floor").unwrap();

            let fun = Function::create(
                sym,
                vec![&mut types::double],
                &mut types::double,
            );

            assert!(fun.is_ok());
        }
    }

    #[test]
    fn test_function_from_pointers() {
        let (mut alloc, state, _process) = setup();
        let name = InkoString::alloc(
            &mut alloc,
            state.permanent_space.string_class(),
            LIBM.to_string(),
        );

        let names = vec![name];
        let lib = Library::from_pointers(&names);

        assert!(lib.is_ok());

        unsafe {
            InkoString::drop(name);
        }
    }

    #[test]
    fn test_function_call() {
        let lib = Library::open(&[LIBM]).unwrap();
        let (mut alloc, state, _process) = setup();
        let arg =
            Float::alloc(&mut alloc, state.permanent_space.float_class(), 3.15);

        unsafe {
            let sym = lib.get("floor").unwrap();
            let fun = Function::create(
                sym,
                vec![&mut types::double],
                &mut types::double,
            )
            .unwrap();

            let res = fun.call(&state, &mut alloc, &[arg]);

            assert!(res.is_ok());
            assert_eq!(Float::read(res.unwrap()), 3.0);
        }
    }

    #[test]
    fn test_pointer_read_and_write() {
        let (mut alloc, state, _process) = setup();

        unsafe {
            let ptr = Pointer::new(calloc(1, 3));
            let kind = InkoPointer::unsigned_int(TYPE_STRING);
            let val = InkoString::alloc(
                &mut alloc,
                state.permanent_space.string_class(),
                "ab".to_string(),
            );

            ptr.write_as(kind, val).unwrap();

            let result = ptr.read_as(&state, &mut alloc, kind);

            free(ptr.inner);

            assert!(result.is_ok());

            let new_string = result.unwrap();

            assert_eq!(InkoString::read(&new_string), "ab");

            InkoString::drop(val);
            InkoString::drop(new_string);
        }
    }
}

#[cfg(test)]
mod tests_for_all_platforms {
    use super::*;

    #[test]
    fn test_library_new_invalid() {
        let lib = Library::open(&["inko-test-1", "inko-test-2"]);

        assert!(lib.is_err());
    }
}
