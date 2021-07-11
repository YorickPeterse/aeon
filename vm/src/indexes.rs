//! Types for indexing various tables such as method tables.
use crate::numeric::u32_from_u16_pair;

/// An index used for accessing methods.
///
/// This type acts as a light wrapper around the real integer type to use for
/// indexes, and signals that the index is in some way already validated (e.g.
/// by the compiler).
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct MethodIndex(u16);

impl MethodIndex {
    /// Returns a new MethodIndex from a raw u16.
    ///
    /// This method is unsafe as it's only supposed to be used with raw values
    /// that have already been validated.
    pub unsafe fn new(index: u16) -> Self {
        Self(index)
    }
}

impl From<MethodIndex> for u16 {
    fn from(index: MethodIndex) -> u16 {
        index.0
    }
}

impl From<MethodIndex> for usize {
    fn from(index: MethodIndex) -> usize {
        index.0 as usize
    }
}

/// An index used for accessing fields.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct FieldIndex(u8);

impl FieldIndex {
    /// Returns a new FieldIndex from a raw u8.
    ///
    /// This method is unsafe as it's only supposed to be used with raw values
    /// that have already been validated.
    pub unsafe fn new(index: u8) -> Self {
        Self(index)
    }
}

impl From<FieldIndex> for u8 {
    fn from(index: FieldIndex) -> u8 {
        index.0
    }
}

impl From<FieldIndex> for usize {
    fn from(index: FieldIndex) -> usize {
        index.0 as usize
    }
}

/// An index used for accessing global variables.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct LocalIndex(u16);

impl LocalIndex {
    /// Returns a new LocalIndex from a raw u16.
    ///
    /// This method is unsafe as it's only supposed to be used with raw values
    /// that have already been validated.
    pub unsafe fn new(index: u16) -> Self {
        Self(index)
    }
}

impl From<LocalIndex> for usize {
    fn from(index: LocalIndex) -> usize {
        index.0 as usize
    }
}

/// An index used for accessing global variables.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct GlobalIndex(u16);

impl GlobalIndex {
    /// Returns a new GlobalIndex from a raw u16.
    ///
    /// This method is unsafe as it's only supposed to be used with raw values
    /// that have already been validated.
    pub unsafe fn new(index: u16) -> Self {
        Self(index)
    }
}

impl From<GlobalIndex> for u16 {
    fn from(index: GlobalIndex) -> u16 {
        index.0
    }
}

impl From<GlobalIndex> for usize {
    fn from(index: GlobalIndex) -> usize {
        index.0 as usize
    }
}

/// An index used for accessing literal values.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct LiteralIndex(usize);

impl LiteralIndex {
    /// Returns a new LiteralIndex from a raw usize.
    ///
    /// This method is unsafe because it doesn't stop you from creating an out
    /// of bounds index.
    pub unsafe fn new(index: usize) -> Self {
        Self(index)
    }

    /// Returns a new LiteralIndex from a raw u16.
    ///
    /// This method is unsafe because it doesn't stop you from creating an out
    /// of bounds index.
    pub unsafe fn from_u16(index: u16) -> Self {
        Self(index as usize)
    }

    /// Returns a LiteralIndex from a pair of u16 values.
    ///
    /// This is useful when a single instruction argument can't represent the
    /// literal index.
    ///
    /// This method is unsafe because it doesn't stop you from creating an out
    /// of bounds index.
    pub unsafe fn wide(a: u16, b: u16) -> Self {
        Self(u32_from_u16_pair(a, b) as usize)
    }
}

impl From<LiteralIndex> for usize {
    fn from(index: LiteralIndex) -> usize {
        index.0
    }
}
