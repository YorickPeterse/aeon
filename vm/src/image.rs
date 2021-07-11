//! Loading of Inko bytecode images.
//!
//! Various chunks of bytecode, such as numbers, are encoded using
//! little-endian. Since most conventional CPUs use little-endian, using the
//! same endianness means we don't have to flip bits around on these CPUs.
use crate::chunk::Chunk;
use crate::config::Config;
use crate::indexes::MethodIndex;
use crate::mem::allocator::Pointer;
use crate::mem::objects::{
    Class, ClassPointer, MethodPointer, ModulePointer, OwnedClass, OwnedModule,
};
use crate::mem::permanent_space::{MethodCounts, PermanentSpace};
use crate::vm::instruction::{Instruction, Opcode};
use crossbeam_channel::bounded;
use crossbeam_utils::thread::scope;
use std::f64;
use std::fs::File;
use std::io::{BufReader, Read};
use std::mem;
use std::str;

macro_rules! read_slice {
    ($stream:expr, $amount:expr) => {{
        let mut buffer: [u8; $amount] = [0; $amount];

        $stream.read_exact(&mut buffer).map_err(|e| e.to_string())?;

        buffer
    }};
}

macro_rules! read_vec {
    ($stream:expr, $amount:expr) => {{
        let mut buffer: Vec<u8> = vec![0; $amount];

        $stream.read_exact(&mut buffer).map_err(|e| e.to_string())?;

        buffer
    }};
}

macro_rules! read_byte {
    ($stream: expr) => {{
        read_slice!($stream, 1)[0]
    }};
}

/// The bytes that every bytecode file must start with.
const SIGNATURE_BYTES: [u8; 4] = [105, 110, 107, 111]; // "inko"

/// The current version of the bytecode format.
const VERSION: u8 = 1;

/// The tag that marks the start of an integer literal.
const LITERAL_INTEGER: u8 = 0;

/// The tag that marks the start of an unsigned integer literal.
const LITERAL_UNSIGNED_INTEGER: u8 = 1;

/// The tag that marks the start of a float literal.
const LITERAL_FLOAT: u8 = 2;

/// The tag that marks the start of a string literal.
const LITERAL_STRING: u8 = 3;

/// The number of bytes to buffer for every read from a bytecode file.
const BUFFER_SIZE: usize = 32 * 1024;

/// A parsed bytecode image.
pub struct Image {
    /// The name of the first module to run.
    pub entry_module: String,

    /// The index of the entry module to run.
    pub entry_method: MethodIndex,

    /// The space to use for allocating permanent objects.
    pub permanent_space: PermanentSpace,

    /// Configuration settings to use when parsing and running bytecode.
    pub config: Config,
}

impl Image {
    /// Loads a bytecode image from a file.
    pub fn load_file(config: Config, path: &str) -> Result<Image, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;

        Self::load(config, &mut BufReader::with_capacity(BUFFER_SIZE, file))
    }

    /// Loads a bytecode image from a stream of bytes.
    pub fn load<R: Read>(
        config: Config,
        stream: &mut R,
    ) -> Result<Image, String> {
        // This is a simply/naive check to make sure we're _probably_ loading an
        // Inko bytecode image; instead of something random like a PNG file.
        if read_slice!(stream, 4) != SIGNATURE_BYTES {
            return Err("The bytecode signature is invalid".to_string());
        }

        let version = read_byte!(stream);

        if version != VERSION {
            return Err(format!(
                "The bytecode version {} is not supported",
                version
            ));
        }

        // Before we can move on, we need to read some counters that specify how
        // many methods the various built-in classes have. These counters are
        // necessary as we can't allocate the built-in classes without them.
        let counts = read_builtin_method_counts(stream)?;
        let space = PermanentSpace::new(counts);

        // The entry module is the first module to run, while the entry method
        // (index) is the method in that module to run.
        let entry_module = read_string(stream)?;
        let entry_method = read_u16(stream)?;

        // Now we can load the bytecode for all modules, including the entry
        // module. The order in which the modules are returned is unspecified.
        let modules =
            read_modules(config.bytecode_threads as usize, &space, stream)?;

        space.add_modules(modules);

        Ok(Image {
            entry_module,
            entry_method: unsafe { MethodIndex::new(entry_method) },
            permanent_space: space,
            config,
        })
    }
}

fn read_builtin_method_counts<R: Read>(
    stream: &mut R,
) -> Result<MethodCounts, String> {
    let int_class = read_u16(stream)?;
    let unsigned_int_class = read_u16(stream)?;
    let float_class = read_u16(stream)?;
    let string_class = read_u16(stream)?;
    let array_class = read_u16(stream)?;
    let method_class = read_u16(stream)?;
    let boolean_class = read_u16(stream)?;
    let nil_class = read_u16(stream)?;
    let byte_array_class = read_u16(stream)?;
    let generator_class = read_u16(stream)?;
    let future_class = read_u16(stream)?;
    let process_client_class = read_u16(stream)?;
    let counts = MethodCounts {
        int_class,
        unsigned_int_class,
        float_class,
        string_class,
        array_class,
        method_class,
        boolean_class,
        nil_class,
        byte_array_class,
        generator_class,
        future_class,
        client_class: process_client_class,
    };

    Ok(counts)
}

fn read_modules<R: Read>(
    concurrency: usize,
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<Vec<OwnedModule>, String> {
    let num_modules = read_u64(stream)? as usize;
    let (in_sender, in_receiver) = bounded::<Vec<u8>>(num_modules);

    scope(|s| {
        let mut modules = Vec::with_capacity(num_modules);
        let mut handles = Vec::with_capacity(concurrency);

        for _ in 0..concurrency {
            let handle = s.spawn(|_| -> Result<Vec<OwnedModule>, String> {
                let mut done = Vec::new();

                while let Ok(chunk) = in_receiver.recv() {
                    done.push(read_module(space, &mut &chunk[..])?);
                }

                Ok(done)
            });

            handles.push(handle);
        }

        for _ in 0..num_modules {
            let amount = read_u64(stream)? as usize;
            let chunk = read_vec!(stream, amount);

            in_sender
                .send(chunk)
                .expect("Failed to send a chunk of bytecode");
        }

        // We need to drop the sender before joining. If we don't, parser
        // threads won't terminate until the end of this scope. But since we are
        // joining those threads, we'd never reach that point.
        drop(in_sender);

        for handle in handles {
            modules
                .append(&mut handle.join().map_err(|e| format!("{:?}", e))??);
        }

        Ok(modules)
    })
    .map_err(|e| format!("{:?}", e))?
}

fn read_module<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<OwnedModule, String> {
    let name = read_string(stream)?;
    let path = read_string(stream)?;
    let num_globals = read_u16(stream)?;
    let num_methods = read_u16(stream)?;
    let literals = read_literals(space, stream)?;

    let mod_class = Class::object(name, Vec::new(), num_methods);
    let module = space.allocate_module(
        OwnedClass::new(mod_class),
        path,
        literals,
        num_globals,
    );

    // TODO: store in globals
    read_classes(space, stream, *module)?;
    read_methods(space, stream, *module, module.class(), num_methods)?;

    Ok(module)
}

fn read_string<R: Read>(stream: &mut R) -> Result<String, String> {
    let size = read_u32(stream)? as usize;
    let buff = read_vec!(stream, size);

    String::from_utf8(buff).map_err(|e| e.to_string())
}

fn read_u8<R: Read>(stream: &mut R) -> Result<u8, String> {
    Ok(read_byte!(stream))
}

fn read_u16<R: Read>(stream: &mut R) -> Result<u16, String> {
    let buff = read_slice!(stream, 2);

    Ok(u16::from_le_bytes(buff))
}

fn read_u32<R: Read>(stream: &mut R) -> Result<u32, String> {
    let buff = read_slice!(stream, 4);

    Ok(u32::from_le_bytes(buff))
}

fn read_u16_as_usize<R: Read>(stream: &mut R) -> Result<usize, String> {
    let buff = read_slice!(stream, 2);

    Ok(u16::from_le_bytes(buff) as usize)
}

fn read_i64<R: Read>(stream: &mut R) -> Result<i64, String> {
    let buff = read_slice!(stream, 8);

    Ok(i64::from_le_bytes(buff))
}

fn read_u64<R: Read>(stream: &mut R) -> Result<u64, String> {
    let buff = read_slice!(stream, 8);

    Ok(u64::from_le_bytes(buff))
}

fn read_f64<R: Read>(stream: &mut R) -> Result<f64, String> {
    let buff = read_slice!(stream, 8);
    let int = u64::from_le_bytes(buff);

    Ok(f64::from_bits(int))
}

fn read_instruction<R: Read>(stream: &mut R) -> Result<Instruction, String> {
    let ins_type: Opcode = unsafe { mem::transmute(read_u8(stream)?) };
    let amount = read_u8(stream)? as usize;
    let mut args = [0, 0, 0, 0, 0, 0];

    if amount > 6 {
        return Err(format!(
            "Instructions are limited to 6 arguments, but {} were given",
            amount
        ));
    }

    for index in 0..amount {
        args[index] = read_u16(stream)?;
    }

    let line = read_u16(stream)?;
    let ins = Instruction::new(ins_type, args, line);

    Ok(ins)
}

fn read_instructions<R: Read>(
    stream: &mut R,
) -> Result<Vec<Instruction>, String> {
    let amount = read_u32(stream)? as usize;
    let mut buff = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(read_instruction(stream)?);
    }

    Ok(buff)
}

fn read_argument_names<R: Read>(stream: &mut R) -> Result<Vec<String>, String> {
    let amount = read_u8(stream)? as usize;
    let mut buff = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(read_string(stream)?);
    }

    Ok(buff)
}

fn read_classes<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
    module: ModulePointer,
) -> Result<(), String> {
    let amount = read_u16_as_usize(stream)?;

    for _ in 0..amount {
        read_class(space, stream, module)?;
    }

    Ok(())
}

fn read_class<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
    module: ModulePointer,
) -> Result<(), String> {
    let kind = read_u8(stream)?;
    let name = read_string(stream)?;
    let fields = read_class_fields(stream)?;
    let num_methods = read_u16(stream)?;
    let class = match kind {
        0 => {
            let new_class = Class::object(name, fields, num_methods);

            unsafe { module.add_class(OwnedClass::new(new_class)) };
            new_class
        }
        1 => {
            let new_class = Class::process(name, fields, num_methods);

            unsafe { module.add_class(OwnedClass::new(new_class)) };
            new_class
        }
        2 => space.int_class(),
        3 => space.unsigned_int_class(),
        4 => space.float_class(),
        5 => space.string_class(),
        6 => space.array_class(),
        7 => space.boolean_class(),
        8 => space.nil_class(),
        9 => space.byte_array_class(),
        10 => space.generator_class(),
        11 => space.future_class(),
        12 => space.main_process_class(),
        13 => space.client_class(),
        14 => space.undefined_class(),
        _ => return Err(format!("The class kind {} is invalid", kind)),
    };

    read_methods(space, stream, module, class, num_methods)
}

fn read_class_fields<R: Read>(stream: &mut R) -> Result<Vec<String>, String> {
    let amount = read_u8(stream)? as usize;
    let mut buff = Vec::with_capacity(amount);

    for _ in 0..amount {
        buff.push(read_string(stream)?);
    }

    Ok(buff)
}

fn read_method<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
    module: ModulePointer,
) -> Result<MethodPointer, String> {
    let hash = read_u32(stream)?;
    let name = read_string(stream)?;
    let file = read_string(stream)?;
    let line = read_u16(stream)?;
    let locals = read_u16(stream)?;
    let registers = read_u16(stream)?;
    let arguments = read_argument_names(stream)?;
    let instructions = read_instructions(stream)?;

    Ok(space.allocate_method(
        hash,
        name,
        file,
        line,
        locals,
        registers,
        arguments,
        instructions,
        module,
    ))
}

fn read_methods<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
    module: ModulePointer,
    class: ClassPointer,
    amount: u16,
) -> Result<(), String> {
    for _ in 0..amount {
        let index = read_u16(stream)?;
        let method = read_method(space, stream, module)?;

        unsafe {
            class.set_method(MethodIndex::new(index), method);
        }
    }

    Ok(())
}

fn read_literals<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<Chunk<Pointer>, String> {
    let amount = read_u32(stream)? as usize;
    let mut buff = Chunk::new(amount);

    for index in 0..amount {
        unsafe {
            buff.set(index, read_literal(space, stream)?);
        }
    }

    Ok(buff)
}

fn read_literal<R: Read>(
    space: &PermanentSpace,
    stream: &mut R,
) -> Result<Pointer, String> {
    let literal_type = read_u8(stream)?;
    let literal = match literal_type {
        LITERAL_INTEGER => space.allocate_int(read_i64(stream)?),
        LITERAL_UNSIGNED_INTEGER => {
            space.allocate_unsigned_int(read_u64(stream)?)
        }
        LITERAL_FLOAT => space.allocate_float(read_f64(stream)?),
        LITERAL_STRING => space.allocate_string(read_string(stream)?),
        _ => {
            return Err(format!("The literal type {} is invalid", literal_type))
        }
    };

    Ok(literal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::{LiteralIndex, MethodIndex};
    use crate::mem::objects::{
        Float, Int, Method, OwnedClass, String as InkoString, UnsignedInt,
    };
    use crate::test::empty_class;
    use crate::vm::instruction::Opcode;
    use std::mem;
    use std::u64;

    fn pack_signature(buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&SIGNATURE_BYTES);
    }

    fn pack_u8(buffer: &mut Vec<u8>, value: u8) {
        buffer.push(value);
    }

    fn pack_u16(buffer: &mut Vec<u8>, value: u16) {
        let num = u16::to_le(value);
        let bytes: [u8; 2] = unsafe { mem::transmute(num) };

        buffer.extend_from_slice(&bytes);
    }

    fn pack_u32(buffer: &mut Vec<u8>, value: u32) {
        let num = u32::to_le(value);
        let bytes: [u8; 4] = unsafe { mem::transmute(num) };

        buffer.extend_from_slice(&bytes);
    }

    fn pack_u64(buffer: &mut Vec<u8>, value: u64) {
        let num = u64::to_le(value);
        let bytes: [u8; 8] = unsafe { mem::transmute(num) };

        buffer.extend_from_slice(&bytes);
    }

    fn pack_i64(buffer: &mut Vec<u8>, value: i64) {
        let num = i64::to_le(value);
        let bytes: [u8; 8] = unsafe { mem::transmute(num) };

        buffer.extend_from_slice(&bytes);
    }

    fn pack_f64(buffer: &mut Vec<u8>, value: f64) {
        pack_u64(buffer, unsafe { mem::transmute(value) });
    }

    fn pack_string(buffer: &mut Vec<u8>, string: &str) {
        pack_u32(buffer, string.len() as u32);

        buffer.extend_from_slice(string.as_bytes());
    }

    fn pack_version(buffer: &mut Vec<u8>) {
        buffer.push(VERSION);
    }

    macro_rules! reader {
        ($buff: expr) => {
            &mut $buff.as_slice()
        };
    }

    #[test]
    fn test_load_empty() {
        let config = Config::new();
        let buffer = Vec::new();
        let output =
            Image::load(config, &mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_load_invalid_signature() {
        let config = Config::new();
        let mut buffer = Vec::new();

        pack_string(&mut buffer, "cats");

        let output =
            Image::load(config, &mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_load_invalid_version() {
        let config = Config::new();
        let mut buffer = Vec::new();

        pack_signature(&mut buffer);

        buffer.push(VERSION + 1);

        let output =
            Image::load(config, &mut BufReader::new(buffer.as_slice()));

        assert!(output.is_err());
    }

    #[test]
    fn test_load_valid() {
        let config = Config::new();
        let mut buffer = Vec::new();

        pack_signature(&mut buffer);
        pack_version(&mut buffer);

        // Built-in method counts
        pack_u16(&mut buffer, 1); // Int
        pack_u16(&mut buffer, 2); // UnsignedInt
        pack_u16(&mut buffer, 4); // Float
        pack_u16(&mut buffer, 5); // String
        pack_u16(&mut buffer, 6); // Array
        pack_u16(&mut buffer, 7); // Method
        pack_u16(&mut buffer, 9); // Boolean
        pack_u16(&mut buffer, 10); // NilType
        pack_u16(&mut buffer, 11); // ByteArray
        pack_u16(&mut buffer, 12); // Generator
        pack_u16(&mut buffer, 13); // Future
        pack_u16(&mut buffer, 14); // Client

        // Entry module and method
        pack_string(&mut buffer, "main");
        pack_u16(&mut buffer, 42);

        // The first (and only module)
        pack_u64(&mut buffer, 1);

        // The number of bytes for this module. This number must be updated
        // according to any changes made to the pack_X lines below.
        pack_u64(&mut buffer, 169);

        pack_string(&mut buffer, "main");
        pack_string(&mut buffer, "main.inko");
        pack_u16(&mut buffer, 2);
        pack_u16(&mut buffer, 1);

        // The module's literals
        pack_u32(&mut buffer, 1);
        pack_u8(&mut buffer, LITERAL_UNSIGNED_INTEGER);
        pack_u64(&mut buffer, 42);

        // Classes defined in the module
        pack_u16(&mut buffer, 1);
        pack_u8(&mut buffer, 0);
        pack_string(&mut buffer, "Counter");
        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "@number");

        // The methods of the class
        pack_u16(&mut buffer, 1);
        pack_u16(&mut buffer, 0);
        pack_u32(&mut buffer, 123);
        pack_string(&mut buffer, "add");
        pack_string(&mut buffer, "main.inko");
        pack_u16(&mut buffer, 4);
        pack_u16(&mut buffer, 1);
        pack_u16(&mut buffer, 2);

        // The method arguments
        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "value");

        // The method instructions
        pack_u32(&mut buffer, 1);
        pack_u8(&mut buffer, Opcode::Return as u8);
        pack_u8(&mut buffer, 1);
        pack_u16(&mut buffer, 2);
        pack_u16(&mut buffer, 4);

        // The module methods
        pack_u16(&mut buffer, 0);
        pack_u32(&mut buffer, 456);
        pack_string(&mut buffer, "new_counter");
        pack_string(&mut buffer, "main.inko");
        pack_u16(&mut buffer, 5);
        pack_u16(&mut buffer, 0);
        pack_u16(&mut buffer, 0);
        pack_u8(&mut buffer, 0);

        // The method instructions
        pack_u32(&mut buffer, 1);
        pack_u8(&mut buffer, Opcode::Return as u8);
        pack_u8(&mut buffer, 1);
        pack_u16(&mut buffer, 2);
        pack_u16(&mut buffer, 5);

        let image = Image::load(config, &mut BufReader::new(buffer.as_slice()))
            .unwrap();

        let entry_method: u16 = image.entry_method.into();
        let perm = &image.permanent_space;

        assert_eq!(perm.int_class.number_of_methods(), 1);
        assert_eq!(perm.unsigned_int_class.number_of_methods(), 2);
        assert_eq!(perm.float_class.number_of_methods(), 4);
        assert_eq!(perm.string_class.number_of_methods(), 5);
        assert_eq!(perm.array_class.number_of_methods(), 6);
        assert_eq!(perm.method_class.number_of_methods(), 7);
        assert_eq!(perm.boolean_class.number_of_methods(), 9);
        assert_eq!(perm.nil_class.number_of_methods(), 10);
        assert_eq!(perm.byte_array_class.number_of_methods(), 11);
        assert_eq!(perm.generator_class.number_of_methods(), 12);
        assert_eq!(perm.future_class.number_of_methods(), 13);
        assert_eq!(perm.client_class.number_of_methods(), 14);

        assert_eq!(image.entry_module, "main".to_string());
        assert_eq!(entry_method, 42);

        let module = perm.get_module("main").unwrap();

        assert_eq!(module.name(), &"main");
        assert_eq!(module.source_path(), &"main.inko");

        unsafe {
            assert_eq!(
                module.get_literal(LiteralIndex::new(0)).as_unsigned_int(),
                42
            );
        }

        assert_eq!(module.classes().len(), 2);

        let module_class = *module.classes()[0];
        let counter_class = *module.classes()[1];

        assert_eq!(module_class.name(), &"main");
        assert_eq!(module_class.number_of_methods(), 1);
        assert_eq!(module_class.fields().len(), 0);

        assert_eq!(counter_class.name(), &"Counter");
        assert_eq!(counter_class.number_of_methods(), 1);
        assert_eq!(counter_class.fields(), &["@number".to_string()]);

        let add_meth = counter_class.get_method(unsafe { MethodIndex::new(0) });
        let new_meth = module_class.get_method(unsafe { MethodIndex::new(0) });

        assert_eq!(add_meth.hash, 123);
        assert_eq!(&add_meth.name, &"add");
        assert_eq!(&add_meth.file, &"main.inko");
        assert_eq!(add_meth.line, 4);
        assert_eq!(add_meth.locals, 1);
        assert_eq!(add_meth.registers, 2);
        assert_eq!(&add_meth.arguments, &["value".to_string()]);
        assert_eq!(add_meth.instructions[0].opcode, Opcode::Return);
        assert_eq!(add_meth.instructions[0].arg(0), 2);
        assert_eq!(add_meth.instructions[0].line, 4);
        assert_eq!(add_meth.module.as_pointer(), module.as_pointer());

        assert_eq!(new_meth.hash, 456);
        assert_eq!(&new_meth.name, &"new_counter");
        assert_eq!(&new_meth.file, &"main.inko");
        assert_eq!(new_meth.line, 5);
        assert_eq!(new_meth.locals, 0);
        assert_eq!(new_meth.registers, 0);
        assert_eq!(new_meth.arguments.len(), 0);
        assert_eq!(new_meth.instructions[0].opcode, Opcode::Return);
        assert_eq!(new_meth.instructions[0].arg(0), 2);
        assert_eq!(new_meth.instructions[0].line, 5);
        assert_eq!(new_meth.instructions[0].line, 5);
        assert_eq!(new_meth.module.as_pointer(), module.as_pointer());
    }

    #[test]
    fn test_read_string() {
        let mut buffer = Vec::new();

        pack_string(&mut buffer, "inko");

        let output = read_string(reader!(buffer)).unwrap();

        assert_eq!(output, "inko".to_string());
    }

    #[test]
    fn test_read_string_too_large() {
        let mut buffer = Vec::new();

        pack_u32(&mut buffer, u32::MAX);

        let output = read_string(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_string_longer_than_size() {
        let mut buffer = Vec::new();

        pack_u32(&mut buffer, 2);
        pack_signature(&mut buffer);

        let output = read_string(reader!(buffer)).unwrap();

        assert_eq!(output, "in".to_string());
    }

    #[test]
    fn test_read_string_invalid_utf8() {
        let mut buffer = Vec::new();

        pack_u32(&mut buffer, 4);
        pack_u8(&mut buffer, 0);
        pack_u8(&mut buffer, 159);
        pack_u8(&mut buffer, 146);
        pack_u8(&mut buffer, 150);

        let output = read_string(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_string_empty() {
        let buffer = Vec::new();
        let output = read_string(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u8() {
        let mut buffer = Vec::new();

        pack_u8(&mut buffer, 2);

        let output = read_u8(reader!(buffer)).unwrap();

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_u8_empty() {
        let buffer = Vec::new();
        let output = read_u8(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_u16() {
        let mut buffer = Vec::new();

        pack_u16(&mut buffer, 2);

        let output = read_u16(reader!(buffer)).unwrap();

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_u16_empty() {
        let buffer = Vec::new();
        let output = read_u16(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_i64() {
        let mut buffer = Vec::new();

        pack_u64(&mut buffer, 2);

        let output = read_i64(reader!(buffer)).unwrap();

        assert_eq!(output, 2);
    }

    #[test]
    fn test_read_i64_empty() {
        let buffer = Vec::new();
        let output = read_i64(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_f64() {
        let mut buffer = Vec::new();

        pack_f64(&mut buffer, 2.123456);

        let output = read_f64(reader!(buffer)).unwrap();

        assert!((2.123456 - output).abs() < 0.00001);
    }

    #[test]
    fn test_read_f64_empty() {
        let buffer = Vec::new();
        let output = read_f64(reader!(buffer));

        assert!(output.is_err());
    }

    #[test]
    fn test_read_instruction() {
        let mut buffer = Vec::new();

        pack_u8(&mut buffer, 0);
        pack_u8(&mut buffer, 1);
        pack_u16(&mut buffer, 6);
        pack_u16(&mut buffer, 2);

        let ins = read_instruction(reader!(buffer)).unwrap();

        assert_eq!(ins.opcode, Opcode::Allocate);
        assert_eq!(ins.arg(0), 6);
        assert_eq!(ins.line, 2);
    }

    #[test]
    fn test_read_instructions() {
        let mut buffer = Vec::new();

        pack_u32(&mut buffer, 1);
        pack_u8(&mut buffer, 0);
        pack_u8(&mut buffer, 1);
        pack_u16(&mut buffer, 6);
        pack_u16(&mut buffer, 2);

        let ins = read_instructions(reader!(buffer)).unwrap();

        assert_eq!(ins.len(), 1);
        assert_eq!(ins[0].opcode, Opcode::Allocate);
        assert_eq!(ins[0].arg(0), 6);
        assert_eq!(ins[0].line, 2);
    }

    #[test]
    fn test_read_argument_names() {
        let mut buffer = Vec::new();

        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "number");

        let args = read_argument_names(reader!(buffer)).unwrap();

        assert_eq!(args.len(), 1);
        assert_eq!(&args[0], &"number");
    }

    #[test]
    fn test_read_class() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(MethodCounts::default());
        let class = empty_class("A");
        let module = perm.allocate_module(
            class,
            "test.inko".to_string(),
            Chunk::new(0),
            0,
        );

        pack_u8(&mut buffer, 0);
        pack_string(&mut buffer, "A");
        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "@number");
        pack_u16(&mut buffer, 0);

        assert!(read_class(&perm, reader!(buffer), *module).is_ok());
        assert_eq!(module.classes().len(), 2);

        let class = *module.classes()[1];

        assert_eq!(class.name(), &"A");
        assert_eq!(class.fields(), &["@number".to_string()]);
        assert_eq!(class.instance_size(), 24);
    }

    #[test]
    fn test_read_class_with_process_class() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(MethodCounts::default());
        let class = empty_class("A");
        let module = perm.allocate_module(
            class,
            "test.inko".to_string(),
            Chunk::new(0),
            0,
        );

        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "A");
        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "@number");
        pack_u16(&mut buffer, 0);

        assert!(read_class(&perm, reader!(buffer), *module).is_ok());
        assert_eq!(module.classes().len(), 2);

        let class = *module.classes()[1];

        assert_eq!(class.name(), &"A");
        assert_eq!(class.fields(), &["@number".to_string()]);
        assert_eq!(class.instance_size(), 48);
    }

    #[test]
    fn test_read_class_with_builtin_class() {
        let mut buffer = Vec::new();
        let mut counts = MethodCounts::default();

        counts.int_class = 1;

        let perm = PermanentSpace::new(counts);
        let class = empty_class("A");
        let module = perm.allocate_module(
            class,
            "test.inko".to_string(),
            Chunk::new(0),
            0,
        );

        pack_u8(&mut buffer, 2);
        pack_string(&mut buffer, "A");
        pack_u8(&mut buffer, 0);
        pack_u16(&mut buffer, 1);

        pack_u16(&mut buffer, 0);
        pack_u32(&mut buffer, 123);
        pack_string(&mut buffer, "add");
        pack_string(&mut buffer, "main.inko");
        pack_u16(&mut buffer, 0);
        pack_u16(&mut buffer, 0);
        pack_u16(&mut buffer, 0);
        pack_u8(&mut buffer, 0);
        pack_u32(&mut buffer, 0);

        assert!(read_class(&perm, reader!(buffer), *module).is_ok());
        assert_eq!(module.classes().len(), 1);
        assert_eq!(perm.int_class.number_of_methods(), 1);
        assert_eq!(
            &perm
                .int_class
                .get_method(unsafe { MethodIndex::new(0) })
                .name,
            &"add"
        );
    }

    #[test]
    fn test_read_class_fields() {
        let mut buffer = Vec::new();

        pack_u8(&mut buffer, 1);
        pack_string(&mut buffer, "@number");

        let output = read_class_fields(reader!(buffer)).unwrap();

        assert_eq!(output, &["@number".to_string()]);
    }

    #[test]
    fn test_read_method() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(MethodCounts::default());
        let class = empty_class("A");
        let module = perm.allocate_module(
            class,
            "test.inko".to_string(),
            Chunk::new(0),
            0,
        );

        pack_u32(&mut buffer, 123);
        pack_string(&mut buffer, "add");
        pack_string(&mut buffer, "main.inko");
        pack_u16(&mut buffer, 1);
        pack_u16(&mut buffer, 2);
        pack_u16(&mut buffer, 3);
        pack_u8(&mut buffer, 0);
        pack_u32(&mut buffer, 0);

        let method = read_method(&perm, reader!(buffer), *module).unwrap();

        assert_eq!(method.hash, 123);
        assert_eq!(&method.name, &"add");
        assert_eq!(&method.file, &"main.inko");
        assert_eq!(method.line, 1);
        assert_eq!(method.locals, 2);
        assert_eq!(method.registers, 3);
        assert_eq!(method.arguments.len(), 0);

        Method::drop(method);
    }

    #[test]
    fn test_read_methods() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(MethodCounts::default());
        let class =
            OwnedClass::new(Class::alloc("A".to_string(), Vec::new(), 2, 0));
        let module = perm.allocate_module(
            class,
            "test.inko".to_string(),
            Chunk::new(0),
            0,
        );

        pack_u16(&mut buffer, 1);
        pack_u32(&mut buffer, 123);
        pack_string(&mut buffer, "add");
        pack_string(&mut buffer, "main.inko");
        pack_u16(&mut buffer, 1);
        pack_u16(&mut buffer, 2);
        pack_u16(&mut buffer, 3);
        pack_u8(&mut buffer, 0);
        pack_u32(&mut buffer, 0);

        assert!(read_methods(
            &perm,
            reader!(buffer),
            *module,
            module.class(),
            1
        )
        .is_ok());

        assert_eq!(
            &module
                .class()
                .get_method(unsafe { MethodIndex::new(1) })
                .name,
            &"add"
        );
    }

    #[test]
    fn test_read_literals() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(MethodCounts::default());

        pack_u32(&mut buffer, 4);
        pack_u8(&mut buffer, LITERAL_INTEGER);
        pack_i64(&mut buffer, -2);
        pack_u8(&mut buffer, LITERAL_UNSIGNED_INTEGER);
        pack_u64(&mut buffer, 2);
        pack_u8(&mut buffer, LITERAL_FLOAT);
        pack_f64(&mut buffer, 2.0);
        pack_u8(&mut buffer, LITERAL_STRING);
        pack_string(&mut buffer, "inko");

        let output = read_literals(&perm, reader!(buffer)).unwrap();

        assert_eq!(output.len(), 4);

        unsafe {
            assert_eq!(Int::read(*output.get(0)), -2);
            assert_eq!(UnsignedInt::read(*output.get(1)), 2);
            assert_eq!(Float::read(*output.get(2)), 2.0);
            assert_eq!(InkoString::read(output.get(3)), "inko");
        }
    }

    #[test]
    fn test_read_literal_invalid() {
        let mut buffer = Vec::new();
        let perm = PermanentSpace::new(MethodCounts::default());

        pack_u8(&mut buffer, 255);

        assert!(read_literal(&perm, reader!(buffer)).is_err());
    }
}
