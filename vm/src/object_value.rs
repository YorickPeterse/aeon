//! Values for Objects
//!
//! Objects need to be able to store values of different types such as floats or
//! strings. The ObjectValue enum can be used for storing such data and
//! operating on it.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::binding::RcBinding;
use crate::block::Block;
use crate::external_functions::ExternalFunction;
use crate::ffi::{Library, Pointer, RcFunction};
use crate::file::File;
use crate::generator::RcGenerator;
use crate::hasher::Hasher;
use crate::immutable_string::ImmutableString;
use crate::module::Module;
use crate::object_pointer::ObjectPointer;
use crate::process::{FutureConsumer, RcProcess};
use crate::socket::Socket;
use num_bigint::BigInt;
use std::mem;
use std::process::Child;

/// Enum for storing different values in an Object.
#[cfg_attr(feature = "cargo-clippy", allow(box_vec))]
pub enum ObjectValue {
    None,
    Float(f64),

    /// Strings use an Arc so they can be sent to other processes without
    /// requiring a full copy of the data.
    String(ArcWithoutWeak<ImmutableString>),

    /// An interned string is a string allocated on the permanent space. For
    /// every unique interned string there is only one object allocated.
    InternedString(ArcWithoutWeak<ImmutableString>),
    Array(Box<Vec<ObjectPointer>>),
    File(Box<File>),
    Block(Box<Block>),
    Binding(RcBinding),

    /// An arbitrary precision integer stored on the heap.
    BigInt(Box<BigInt>),

    /// A heap allocated integer that doesn't fit in a tagged pointer, but is
    /// too small for a BigInt.
    Integer(i64),

    /// A heap allocator hasher used for hashing objects.
    Hasher(Box<Hasher>),

    /// An Array of bytes, typically produced by reading from a stream of sorts.
    ByteArray(Box<Vec<u8>>),

    /// A C library opened using the FFI.
    Library(Library),

    /// A C function to call using the FFI.
    Function(RcFunction),

    /// A raw C pointer.
    Pointer(Pointer),

    /// A lightweight Inko process.
    Process(RcProcess),

    /// A nonblocking socket.
    Socket(Box<Socket>),

    /// An Inko module.
    Module(ArcWithoutWeak<Module>),

    /// A generator that can be resumed.
    Generator(RcGenerator),

    /// An external function.
    ExternalFunction(ExternalFunction),

    /// An OS command.
    Command(Box<Child>),

    /// A future created in response to a message being sent.
    Future(Box<FutureConsumer>),
}

impl ObjectValue {
    pub fn is_none(&self) -> bool {
        match *self {
            ObjectValue::None => true,
            _ => false,
        }
    }

    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn is_float(&self) -> bool {
        match *self {
            ObjectValue::Float(_) => true,
            _ => false,
        }
    }

    pub fn is_array(&self) -> bool {
        match *self {
            ObjectValue::Array(_) => true,
            _ => false,
        }
    }

    pub fn is_string(&self) -> bool {
        match *self {
            ObjectValue::String(_) | ObjectValue::InternedString(_) => true,
            _ => false,
        }
    }

    pub fn is_interned_string(&self) -> bool {
        match *self {
            ObjectValue::InternedString(_) => true,
            _ => false,
        }
    }

    pub fn is_file(&self) -> bool {
        match *self {
            ObjectValue::File(_) => true,
            _ => false,
        }
    }

    pub fn is_block(&self) -> bool {
        match *self {
            ObjectValue::Block(_) => true,
            _ => false,
        }
    }

    pub fn is_binding(&self) -> bool {
        match *self {
            ObjectValue::Binding(_) => true,
            _ => false,
        }
    }

    pub fn is_integer(&self) -> bool {
        match *self {
            ObjectValue::Integer(_) => true,
            _ => false,
        }
    }

    pub fn is_bigint(&self) -> bool {
        match *self {
            ObjectValue::BigInt(_) => true,
            _ => false,
        }
    }

    pub fn is_library(&self) -> bool {
        match *self {
            ObjectValue::Library(_) => true,
            _ => false,
        }
    }

    pub fn as_float(&self) -> Result<f64, String> {
        match *self {
            ObjectValue::Float(val) => Ok(val),
            _ => Err("as_float called non a non float value".to_string()),
        }
    }

    pub fn as_array(&self) -> Result<&Vec<ObjectPointer>, String> {
        match *self {
            ObjectValue::Array(ref val) => Ok(val),
            _ => Err("as_array called non a non array value".to_string()),
        }
    }

    pub fn as_array_mut(&mut self) -> Result<&mut Vec<ObjectPointer>, String> {
        match *self {
            ObjectValue::Array(ref mut val) => Ok(val),
            _ => Err("as_array_mut called on a non array".to_string()),
        }
    }

    pub fn as_byte_array(&self) -> Result<&Vec<u8>, String> {
        match *self {
            ObjectValue::ByteArray(ref val) => Ok(val),
            _ => {
                Err("as_byte_array called non a non byte array value"
                    .to_string())
            }
        }
    }

    pub fn as_byte_array_mut(&mut self) -> Result<&mut Vec<u8>, String> {
        match *self {
            ObjectValue::ByteArray(ref mut val) => Ok(val),
            _ => {
                Err("as_byte_array_mut called on a non byte array".to_string())
            }
        }
    }

    pub fn as_string(&self) -> Result<&ImmutableString, String> {
        match *self {
            ObjectValue::String(ref val) => Ok(val),
            ObjectValue::InternedString(ref val) => Ok(val),
            _ => {
                Err("ObjectValue::as_string() called on a non string"
                    .to_string())
            }
        }
    }

    pub fn as_file(&self) -> Result<&File, String> {
        match *self {
            ObjectValue::File(ref val) => Ok(val),
            _ => Err("ObjectValue::as_file() called on a non file".to_string()),
        }
    }

    pub fn as_file_mut(&mut self) -> Result<&mut File, String> {
        match *self {
            ObjectValue::File(ref mut val) => Ok(val),
            _ => {
                Err("ObjectValue::as_file_mut() called on a non file"
                    .to_string())
            }
        }
    }

    pub fn as_block(&self) -> Result<&Block, String> {
        match *self {
            ObjectValue::Block(ref val) => Ok(val),
            _ => Err("ObjectValue::as_block() called on a non block object"
                .to_string()),
        }
    }

    pub fn as_binding(&self) -> Result<RcBinding, String> {
        match *self {
            ObjectValue::Binding(ref val) => Ok(val.clone()),
            _ => Err("ObjectValue::as_binding() called non a non Binding"
                .to_string()),
        }
    }

    pub fn as_bigint(&self) -> Result<&BigInt, String> {
        match *self {
            ObjectValue::BigInt(ref val) => Ok(val),
            _ => {
                Err("ObjectValue::as_bigint() called on a non BigInt"
                    .to_string())
            }
        }
    }

    pub fn as_integer(&self) -> Result<i64, String> {
        match *self {
            ObjectValue::Integer(val) => Ok(val),
            _ => {
                Err("ObjectValue::as_integer() called on a non integer"
                    .to_string())
            }
        }
    }

    pub fn as_hasher_mut(&mut self) -> Result<&mut Hasher, String> {
        match *self {
            ObjectValue::Hasher(ref mut val) => Ok(val),
            _ => Err("ObjectValue::as_hasher_mut() called on a non hasher"
                .to_string()),
        }
    }

    pub fn as_hasher(&self) -> Result<&Hasher, String> {
        match *self {
            ObjectValue::Hasher(ref val) => Ok(val),
            _ => {
                Err("ObjectValue::as_hasher() called on a non hasher"
                    .to_string())
            }
        }
    }

    pub fn as_library(&self) -> Result<&Library, String> {
        match *self {
            ObjectValue::Library(ref lib) => Ok(lib),
            _ => {
                Err("ObjectValue::as_library() called on a non library"
                    .to_string())
            }
        }
    }

    pub fn as_function(&self) -> Result<&RcFunction, String> {
        match *self {
            ObjectValue::Function(ref fun) => Ok(fun),
            _ => Err("ObjectValue::as_function() called on a non function"
                .to_string()),
        }
    }

    pub fn as_pointer(&self) -> Result<Pointer, String> {
        match *self {
            ObjectValue::Pointer(ptr) => Ok(ptr),
            _ => {
                Err("ObjectValue::as_pointer() called on a non pointer"
                    .to_string())
            }
        }
    }

    pub fn as_process(&self) -> Result<&RcProcess, String> {
        match *self {
            ObjectValue::Process(ref proc) => Ok(proc),
            _ => {
                Err("ObjectValue::as_process() called on a non process"
                    .to_string())
            }
        }
    }

    pub fn as_socket(&self) -> Result<&Socket, String> {
        match *self {
            ObjectValue::Socket(ref sock) => Ok(sock),
            _ => {
                Err("ObjectValue::as_socket() called on a non socket"
                    .to_string())
            }
        }
    }

    pub fn as_socket_mut(&mut self) -> Result<&mut Socket, String> {
        match *self {
            ObjectValue::Socket(ref mut sock) => Ok(sock),
            _ => Err("ObjectValue::as_socket_mut() called on a non socket"
                .to_string()),
        }
    }

    pub fn as_module(&self) -> Result<&ArcWithoutWeak<Module>, String> {
        match *self {
            ObjectValue::Module(ref module) => Ok(module),
            _ => {
                Err("ObjectValue::as_module() called on a non module"
                    .to_string())
            }
        }
    }

    pub fn as_module_mut(
        &mut self,
    ) -> Result<&mut ArcWithoutWeak<Module>, String> {
        match *self {
            ObjectValue::Module(ref mut module) => Ok(module),
            _ => Err("ObjectValue::as_module_mut() called on a non module"
                .to_string()),
        }
    }

    pub fn as_generator(&self) -> Result<&RcGenerator, String> {
        match *self {
            ObjectValue::Generator(ref gen) => Ok(gen),
            _ => Err("ObjectValue::as_generator() called on a non generator"
                .to_string()),
        }
    }

    pub fn as_external_function(&self) -> Result<ExternalFunction, String> {
        match *self {
            ObjectValue::ExternalFunction(fun) => Ok(fun),
            _ => Err("ObjectValue::as_external_function() called on a non external function"
                .to_string()),
        }
    }

    pub fn as_command(&self) -> Result<&Child, String> {
        match *self {
            ObjectValue::Command(ref cmd) => Ok(cmd),
            _ => {
                Err("ObjectValue::as_command() called on a non command"
                    .to_string())
            }
        }
    }

    pub fn as_command_mut(&mut self) -> Result<&mut Child, String> {
        match *self {
            ObjectValue::Command(ref mut cmd) => Ok(cmd),
            _ => Err("ObjectValue::as_command_mut() called on a non command"
                .to_string()),
        }
    }

    pub fn as_future(&self) -> Result<&FutureConsumer, String> {
        match *self {
            ObjectValue::Future(ref fut) => Ok(fut),
            _ => {
                Err("ObjectValue::as_future() called on a non future"
                    .to_string())
            }
        }
    }

    pub fn take(&mut self) -> ObjectValue {
        mem::replace(self, ObjectValue::None)
    }

    /// Returns true if this value should be deallocated explicitly.
    pub fn should_deallocate_native(&self) -> bool {
        match *self {
            ObjectValue::None => false,
            _ => true,
        }
    }

    pub fn is_immutable(&self) -> bool {
        match *self {
            ObjectValue::Float(_)
            | ObjectValue::Integer(_)
            | ObjectValue::String(_)
            | ObjectValue::BigInt(_)
            | ObjectValue::InternedString(_)
            | ObjectValue::Hasher(_)
            | ObjectValue::ByteArray(_)
            | ObjectValue::Library(_)
            | ObjectValue::Function(_)
            | ObjectValue::Pointer(_)
            | ObjectValue::Process(_)
            | ObjectValue::Socket(_)
            | ObjectValue::Generator(_)
            | ObjectValue::Command(_)
            | ObjectValue::ExternalFunction(_)
            | ObjectValue::Future(_) => true,
            _ => false,
        }
    }

    pub fn name(&self) -> &str {
        match *self {
            ObjectValue::None => "Object",
            ObjectValue::Float(_) => "Float",
            ObjectValue::String(_) | ObjectValue::InternedString(_) => "String",
            ObjectValue::Array(_) => "Array",
            ObjectValue::File(_) => "File",
            ObjectValue::Block(_) => "Block",
            ObjectValue::Binding(_) => "Binding",
            ObjectValue::BigInt(_) => "BigInteger",
            ObjectValue::Integer(_) => "Integer",
            ObjectValue::Hasher(_) => "Hasher",
            ObjectValue::ByteArray(_) => "ByteArray",
            ObjectValue::Library(_) => "Library",
            ObjectValue::Function(_) => "Function",
            ObjectValue::Pointer(_) => "Pointer",
            ObjectValue::Process(_) => "Process",
            ObjectValue::Socket(_) => "Socket",
            ObjectValue::Module(_) => "Module",
            ObjectValue::Generator(_) => "Generator",
            ObjectValue::ExternalFunction(_) => "BuiltinFunction",
            ObjectValue::Command(_) => "Command",
            ObjectValue::Future(_) => "Future",
        }
    }

    pub fn close(&mut self) {
        match *self {
            ObjectValue::File(ref mut thing) => thing.close(),
            ObjectValue::Library(ref mut thing) => thing.close(),
            ObjectValue::Socket(ref mut thing) => thing.close(),
            _ => {}
        }
    }
}

pub fn none() -> ObjectValue {
    ObjectValue::None
}

pub fn float(value: f64) -> ObjectValue {
    ObjectValue::Float(value)
}

pub fn string(value: String) -> ObjectValue {
    immutable_string(ImmutableString::from(value))
}

pub fn immutable_string(value: ImmutableString) -> ObjectValue {
    ObjectValue::String(ArcWithoutWeak::new(value))
}

pub fn interned_string(value: ImmutableString) -> ObjectValue {
    ObjectValue::InternedString(ArcWithoutWeak::new(value))
}

pub fn array(value: Vec<ObjectPointer>) -> ObjectValue {
    ObjectValue::Array(Box::new(value))
}

pub fn file(value: File) -> ObjectValue {
    ObjectValue::File(Box::new(value))
}

pub fn block(value: Block) -> ObjectValue {
    ObjectValue::Block(Box::new(value))
}

pub fn binding(value: RcBinding) -> ObjectValue {
    ObjectValue::Binding(value)
}

pub fn bigint(value: BigInt) -> ObjectValue {
    ObjectValue::BigInt(Box::new(value))
}

pub fn integer(value: i64) -> ObjectValue {
    ObjectValue::Integer(value)
}

pub fn hasher(value: Hasher) -> ObjectValue {
    ObjectValue::Hasher(Box::new(value))
}

pub fn byte_array(value: Vec<u8>) -> ObjectValue {
    ObjectValue::ByteArray(Box::new(value))
}

pub fn library(value: Library) -> ObjectValue {
    ObjectValue::Library(value)
}

pub fn function(value: RcFunction) -> ObjectValue {
    ObjectValue::Function(value)
}

pub fn pointer(value: Pointer) -> ObjectValue {
    ObjectValue::Pointer(value)
}

pub fn process(value: RcProcess) -> ObjectValue {
    ObjectValue::Process(value)
}

pub fn socket(value: Socket) -> ObjectValue {
    ObjectValue::Socket(Box::new(value))
}

pub fn module(value: ArcWithoutWeak<Module>) -> ObjectValue {
    ObjectValue::Module(value)
}

pub fn generator(value: RcGenerator) -> ObjectValue {
    ObjectValue::Generator(value)
}

pub fn external_function(value: ExternalFunction) -> ObjectValue {
    ObjectValue::ExternalFunction(value)
}

pub fn command(command: Child) -> ObjectValue {
    ObjectValue::Command(Box::new(command))
}

pub fn future(value: FutureConsumer) -> ObjectValue {
    ObjectValue::Future(Box::new(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Binding;
    use crate::block::Block;
    use crate::compiled_code::CompiledCode;
    use crate::config::Config;
    use crate::ffi::Library;
    use crate::object_pointer::ObjectPointer;
    use crate::vm::state::{RcState, State};

    #[cfg(target_os = "macos")]
    const LIBM: &'static str = "libm.dylib";

    #[cfg(target_os = "linux")]
    const LIBM: &'static str = "libm.so.6";

    #[cfg(target_os = "windows")]
    const LIBM: &'static str = "msvcrt.dll";

    fn null_device_path() -> &'static str {
        if cfg!(windows) {
            "nul"
        } else {
            "/dev/null"
        }
    }

    fn state() -> RcState {
        State::with_rc(Config::new(), &[])
    }

    #[test]
    fn test_is_none() {
        assert!(ObjectValue::None.is_none());
        assert_eq!(ObjectValue::Float(10.0).is_none(), false);
    }

    #[test]
    fn test_is_some() {
        assert_eq!(ObjectValue::None.is_some(), false);
        assert!(ObjectValue::Float(1.0).is_some());
    }

    #[test]
    fn test_is_float() {
        assert!(ObjectValue::Float(10.5).is_float());
        assert_eq!(ObjectValue::None.is_float(), false);
    }

    #[test]
    fn test_is_array() {
        assert!(ObjectValue::Array(Box::new(Vec::new())).is_array());
        assert_eq!(ObjectValue::None.is_array(), false);
    }

    #[test]
    fn test_is_string() {
        let string = string("a".to_string());

        assert!(string.is_string());
        assert_eq!(ObjectValue::None.is_string(), false);
    }

    #[test]
    fn test_is_string_with_interned_string() {
        let string = interned_string(ImmutableString::from("a".to_string()));

        assert!(string.is_string());
    }

    #[test]
    fn test_is_interned_string() {
        let string = interned_string(ImmutableString::from("a".to_string()));

        assert!(string.is_interned_string());
    }

    #[test]
    fn test_is_interned_string_with_regular_string() {
        let string = immutable_string(ImmutableString::from("a".to_string()));

        assert_eq!(string.is_interned_string(), false);
    }

    #[test]
    fn test_is_file() {
        let state = state();
        let path = state.intern_string(null_device_path().to_string());
        let file = Box::new(File::read_only(path).unwrap());

        assert!(ObjectValue::File(file).is_file());
        assert_eq!(ObjectValue::None.is_file(), false);
    }

    #[test]
    fn test_is_block() {
        let state = state();
        let name = state.intern_string("a".to_string());
        let path = state.intern_string("a.inko".to_string());
        let code = CompiledCode::new(name, path, 1, Vec::new());
        let module = Module::new(name, code, Vec::new());
        let block =
            Block::new(module.code(), None, ObjectPointer::integer(1), &module);

        assert!(ObjectValue::Block(Box::new(block)).is_block());
        assert_eq!(ObjectValue::None.is_block(), false);
    }

    #[test]
    fn test_is_binding() {
        let binding = Binding::new(0, ObjectPointer::integer(10), None);

        assert!(ObjectValue::Binding(binding).is_binding());
        assert_eq!(ObjectValue::None.is_binding(), false);
    }

    #[test]
    fn test_as_float_without_float() {
        assert!(ObjectValue::None.as_float().is_err());
    }

    #[test]
    fn test_as_float_with_float() {
        let result = ObjectValue::Float(10.5).as_float();

        assert!(result.is_ok());
        assert!(result.unwrap() == 10.5);
    }

    #[test]
    fn test_as_array_without_array() {
        assert!(ObjectValue::None.as_array().is_err());
    }

    #[test]
    fn test_as_array_with_array() {
        let array = Box::new(vec![ObjectPointer::null()]);
        let value = ObjectValue::Array(array);
        let result = value.as_array();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_as_array_mut_without_array() {
        assert!(ObjectValue::None.as_array_mut().is_err());
    }

    #[test]
    fn test_as_array_mut_with_array() {
        let array = Box::new(vec![ObjectPointer::null()]);
        let mut value = ObjectValue::Array(array);
        let result = value.as_array_mut();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_as_string_without_string() {
        assert!(ObjectValue::None.as_string().is_err());
    }

    #[test]
    fn test_as_string_with_string() {
        let value = string("test".to_string());
        let result = value.as_string();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_slice(), "test");
    }

    #[test]
    fn test_as_file_without_file() {
        assert!(ObjectValue::None.as_file().is_err());
    }

    #[test]
    fn test_as_file_with_file() {
        let state = state();
        let path = state.intern_string(null_device_path().to_string());
        let file = Box::new(File::read_only(path).unwrap());
        let value = ObjectValue::File(file);
        let result = value.as_file();

        assert!(result.is_ok());
    }

    #[test]
    fn test_as_file_mut_without_file() {
        assert!(ObjectValue::None.as_file_mut().is_err());
    }

    #[test]
    fn test_as_file_mut_with_file() {
        let state = state();
        let path = state.intern_string(null_device_path().to_string());
        let file = Box::new(File::read_only(path).unwrap());
        let mut value = ObjectValue::File(file);
        let result = value.as_file_mut();

        assert!(result.is_ok());
    }

    #[test]
    fn test_as_block_without_block() {
        assert!(ObjectValue::None.as_block().is_err());
    }

    #[test]
    fn test_as_block_with_block() {
        let state = state();
        let name = state.intern_string("a".to_string());
        let path = state.intern_string("a.inko".to_string());
        let code = CompiledCode::new(name, path, 1, Vec::new());
        let module = Module::new(name, code, Vec::new());
        let block =
            Block::new(module.code(), None, ObjectPointer::integer(1), &module);

        let value = ObjectValue::Block(Box::new(block));

        assert!(value.as_block().is_ok());
    }

    #[test]
    fn test_as_binding_without_binding() {
        assert!(ObjectValue::None.as_binding().is_err());
    }

    #[test]
    fn test_as_binding_with_binding() {
        let pointer = ObjectPointer::integer(5);
        let binding = Binding::new(1, ObjectPointer::integer(10), None);

        binding.set_local(0, pointer);

        let result = ObjectValue::Binding(binding).as_binding();

        assert!(result.is_ok());
        assert!(result.unwrap().get_local(0) == pointer);
    }

    #[test]
    fn test_take() {
        let mut val1 = ObjectValue::Float(5.0);
        let val2 = val1.take();

        assert!(val1.is_none());
        assert!(val2.is_float());
        assert_eq!(val2.as_float().unwrap(), 5.0);
    }

    #[test]
    fn test_none() {
        assert!(none().is_none());
    }

    #[test]
    fn test_float() {
        assert!(float(10.5).is_float());
    }

    #[test]
    fn test_string() {
        assert!(string("a".to_string()).is_string());
    }

    #[test]
    fn test_array() {
        assert!(array(Vec::new()).is_array());
    }

    #[test]
    fn test_file() {
        let state = state();
        let path = state.intern_string(null_device_path().to_string());
        let f = File::read_only(path).unwrap();

        assert!(file(f).is_file());
    }

    #[test]
    fn test_block() {
        let state = state();
        let name = state.intern_string("a".to_string());
        let path = state.intern_string("a.inko".to_string());
        let code = CompiledCode::new(name, path, 1, Vec::new());
        let module = Module::new(name, code, Vec::new());
        let blk =
            Block::new(module.code(), None, ObjectPointer::null(), &module);

        assert!(block(blk).is_block());
    }

    #[test]
    fn test_binding() {
        let b = Binding::new(0, ObjectPointer::integer(10), None);

        assert!(binding(b).is_binding());
    }

    #[test]
    fn test_is_immutable() {
        assert!(string("a".to_string()).is_immutable());
        assert!(float(10.5).is_immutable());
        assert!(interned_string(ImmutableString::from("a".to_string()))
            .is_immutable());
    }

    #[test]
    fn test_as_byte_array_without_byte_array() {
        assert!(ObjectValue::None.as_byte_array().is_err());
    }

    #[test]
    fn test_as_byte_array_with_byte_array() {
        let byte_array = Box::new(vec![1]);
        let value = ObjectValue::ByteArray(byte_array);
        let result = value.as_byte_array();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_as_byte_array_mut_without_byte_array() {
        assert!(ObjectValue::None.as_byte_array_mut().is_err());
    }

    #[test]
    fn test_as_byte_array_mut_with_byte_array() {
        let byte_array = Box::new(vec![1]);
        let mut value = ObjectValue::ByteArray(byte_array);
        let result = value.as_byte_array_mut();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    #[cfg(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "macos"
    ))]
    fn test_is_library() {
        let lib = library(Library::open(&[LIBM]).unwrap());

        assert!(lib.is_library());
    }

    #[test]
    #[cfg(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "macos"
    ))]
    fn test_as_library() {
        let lib = library(Library::open(&[LIBM]).unwrap());

        assert!(lib.as_library().is_ok());
    }

    #[test]
    fn test_name() {
        assert_eq!(ObjectValue::None.name(), "Object");
        assert_eq!(ObjectValue::Integer(14).name(), "Integer");
    }
}
