//! A registry of external functions that can be called in Inko source code.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;
use ahash::AHashMap;
use std::io::Read;

/// Defines a setup() function that registers all the given external functions.
macro_rules! register {
    ($($name:ident),*) => {
        pub fn setup(
            functions: &mut crate::external_functions::ExternalFunctions
        ) -> Result<(), String> {
            $(
                functions.add(stringify!($name), $name)?;
            )*
            Ok(())
        }
    }
}

mod array;
mod blocks;
mod byte_array;
mod child_process;
mod env;
mod ffi;
mod float;
mod fs;
mod future;
mod generator;
mod hasher;
mod integer;
mod modules;
mod object;
mod process;
mod random;
mod socket;
mod stdio;
mod string;
mod time;

/// A external function that can be called from Inko source code.
pub type ExternalFunction = fn(
    &State,
    &mut BumpAllocator,
    ServerPointer,
    GeneratorPointer,
    &[Pointer],
) -> Result<Pointer, RuntimeError>;

/// Reads a number of bytes from a buffer into a Vec.
pub fn read_into<T: Read>(
    stream: &mut T,
    output: &mut Vec<u8>,
    size: u64,
) -> Result<u64, RuntimeError> {
    let read = if size > 0 {
        stream.take(size).read_to_end(output)?
    } else {
        stream.read_to_end(output)?
    };

    Ok(read as u64)
}

/// A collection of external functions.
pub struct ExternalFunctions {
    mapping: AHashMap<String, ExternalFunction>,
}

impl ExternalFunctions {
    /// Creates a collection of external functions and registers all functions
    /// that Inko ships with.
    pub fn setup() -> Result<Self, String> {
        let mut instance = Self::new();

        random::setup(&mut instance)?;
        fs::setup(&mut instance)?;
        stdio::setup(&mut instance)?;
        env::setup(&mut instance)?;
        time::setup(&mut instance)?;
        hasher::setup(&mut instance)?;
        blocks::setup(&mut instance)?;
        ffi::setup(&mut instance)?;
        modules::setup(&mut instance)?;
        socket::setup(&mut instance)?;
        process::setup(&mut instance)?;
        array::setup(&mut instance)?;
        byte_array::setup(&mut instance)?;
        float::setup(&mut instance)?;
        object::setup(&mut instance)?;
        integer::setup(&mut instance)?;
        string::setup(&mut instance)?;
        child_process::setup(&mut instance)?;
        generator::setup(&mut instance)?;
        future::setup(&mut instance)?;

        Ok(instance)
    }

    /// Creates a new empty collection of external functions.
    pub fn new() -> Self {
        Self {
            mapping: AHashMap::default(),
        }
    }

    /// Adds a new external function with the given name.
    pub fn add<I: Into<String>>(
        &mut self,
        name: I,
        function: ExternalFunction,
    ) -> Result<(), String> {
        let name: String = name.into();

        if self.mapping.contains_key(&name) {
            return Err(format!(
                "The external function {} is already defined",
                name
            ));
        }

        self.mapping.insert(name, function);
        Ok(())
    }

    /// Looks up a external function by its name.
    pub fn get(&self, name: &str) -> Result<ExternalFunction, String> {
        self.mapping.get(name).cloned().ok_or_else(|| {
            format!("The external function {} is undefined", name)
        })
    }
}
