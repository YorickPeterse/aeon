//! Allocators for Inko objects.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::config::Config;
use parking_lot::Mutex;
use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::mem::size_of;
use std::ops::{Deref, DerefMut};
use std::ptr::{drop_in_place, NonNull};
use std::sync::atomic::{AtomicU8, Ordering};

/// The size of a line, in bytes.
///
/// If this ever needs to be changed, don't forget to also change the alignment
/// of `BlockHeader`.
const LINE_SIZE: usize = 128;

/// The number of lines per block.
const LINES_PER_BLOCK: usize = 256;

/// The size of a block, in bytes.
const BLOCK_SIZE: usize = LINE_SIZE * LINES_PER_BLOCK;

/// A byte mask to apply to go from a pointer to the start of its line.
const LINE_START_MASK: isize = !(LINE_SIZE as isize - 1);

/// A byte mask to apply to go from a pointer to the start of its block.
const BLOCK_START_MASK: isize = !(BLOCK_SIZE as isize - 1);

/// The first line that can be allocated into.
const FIRST_USABLE_LINE: u8 =
    ((size_of::<BlockHeader>() + LINE_SIZE - 1) / LINE_SIZE) as u8;

/// The offset (relative to a block) the first object can be allocated into.
const FIRST_ALLOCATION_OFFSET: usize = FIRST_USABLE_LINE as usize * LINE_SIZE;

/// The number of usable lines in a block.
///
/// Due to the number of reserved lines, this always fits in a single byte.
const USABLE_LINES_PER_BLOCK: u8 =
    (LINES_PER_BLOCK - (FIRST_USABLE_LINE as usize + 1)) as u8;

/// The header of a block.
///
/// Each block reserves a bit of space at the start for this header.
///
/// Blocks in turn are linked together as a doubly-linked list, without the need
/// for extra nodes/indirection.
pub struct BlockHeader {
    /// The pointer bump allocate the next object into.
    ptr: *mut u8,

    /// The end of the hole to allocate into.
    ///
    /// This pointer points toward the _end_ of a hole, not the start of the
    /// last line in a hole. As such, no objects can be allocated into this
    /// pointer.
    end: *mut u8,

    /// The number of reusable lines in this block.
    ///
    /// While the number of lines in a block is 256, we still use an (atomic) u8
    /// here. This is deliberate: the first 3 lines are reserved, meaning this
    /// counter can never go above 253; which fits in a single byte.
    reusable_lines: AtomicU8,

    /// A pointer to the previous block.
    previous_block: Option<BlockPointer>,

    /// A pointer to the next block to allocate objects into; if any.
    next_block: Option<Block>,

    /// A byte map that tracks the number of live objects per line.
    line_map: [AtomicU8; LINES_PER_BLOCK],
}

impl BlockHeader {
    fn line_object_count(&self, index: u8) -> u8 {
        unsafe {
            self.line_map
                .get_unchecked(index as usize)
                .load(Ordering::Acquire)
        }
    }

    fn line_is_used(&self, index: u8) -> bool {
        self.line_object_count(index) > 0
    }

    fn line_is_free(&self, index: u8) -> bool {
        self.line_object_count(index) == 0
    }

    fn increase_line_object_count(&self, index: u8) {
        unsafe {
            self.line_map
                .get_unchecked(index as usize)
                .fetch_add(1, Ordering::AcqRel);
        }
    }

    fn reduce_line_object_count(&self, index: u8) -> bool {
        unsafe {
            self.line_map
                .get_unchecked(index as usize)
                .fetch_sub(1, Ordering::AcqRel)
                == 1
        }
    }

    fn increase_reusable_lines(&self, amount: u8) {
        self.reusable_lines.fetch_add(amount, Ordering::AcqRel);
    }
}

/// An unowned pointer to a block to allocate into.
///
/// Blocks use a fixed size (per `BLOCK_SIZE`), and are aligned to their size.
/// By aligning the blocks to their size, it's trivial to get the block header
/// for a given object pointer.
///
/// The first three lines are reserved for the block header. Reserving the first
/// few lines allows for embedding of metadata, without the need for extra
/// indirection.
///
/// The layout of our heap is based on the heap layout of the Immix garbage
/// collector, as described in the paper "Immix: A Mark-Region Garbage Collector
/// with Space Efficiency, Fast Collection, and Mutator Performance" by Stephen
/// M. Blackburn and Kathryn S. McKinley.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(transparent)]
pub struct BlockPointer {
    /// The raw data of this block.
    ptr: NonNull<u8>,
}

unsafe impl Send for BlockPointer {}

impl BlockPointer {
    /// Bump allocates into the current hole.
    ///
    /// This method assumes the size is already rounded to a multiple of 8. It
    /// can make this assumption, because when classes are defined the sizes of
    /// their instances are calculated; including alignment.
    ///
    /// Objects may span lines, but not blocks. When an object spans lines, all
    /// the covered lines have their object counts incremented.
    fn allocate(self, size: usize) -> Option<NonNull<u8>> {
        debug_assert!(
            size <= (BLOCK_SIZE - LINE_SIZE),
            "failed to allocate {} bytes in a single block",
            size
        );

        let header = self.header_mut();
        let ptr = header.ptr;
        let new_ptr = unsafe { ptr.add(size) };

        if new_ptr > header.end {
            return None;
        }

        header.ptr = new_ptr;

        let first_line = line_index_for_pointer(ptr);
        let mut last_line = line_index_for_pointer(new_ptr);

        if last_line > first_line && new_ptr == line_start_pointer(new_ptr) {
            last_line -= 1;
        }

        for line in first_line..=last_line {
            header.increase_line_object_count(line);
        }

        unsafe { Some(NonNull::new_unchecked(ptr)) }
    }

    /// Advances the bump cursor to the next hole.
    ///
    /// This method returns true if a hole is found.
    fn find_hole(self) -> bool {
        let header = self.header();

        if header.ptr == self.end_address() {
            return false;
        }

        // We start at the next line. If we started at the current line, and
        // still had space left (but not enough for an allocation), we'd not
        // advance the bump cursor.
        self.find_hole_starting_at(line_index_for_pointer(header.ptr) + 1)
    }

    /// Advances the bump pointer to the next hole, starting at the given line.
    ///
    /// This method returns `true` if a hole is found.
    fn find_hole_starting_at(self, line_index: u8) -> bool {
        debug_assert!(
            line_index >= FIRST_USABLE_LINE,
            "The line {} is below the minimum {}",
            line_index,
            FIRST_USABLE_LINE
        );

        let mut line = line_index as usize;
        let lines = self.ptr.as_ptr();
        let mut found = false;
        let mut header = self.header_mut();

        while line < LINES_PER_BLOCK {
            if header.line_is_free(line as u8) {
                header.ptr = unsafe { lines.add(LINE_SIZE * line) };
                found = true;

                break;
            }

            line += 1;
        }

        if !found {
            // When no hole is found, we can leave the cursor, start and end
            // as-is. This because we skip the block for future allocations.
            // Not updating the fields saves us redundant field writes in this
            // case.
            return false;
        }

        while line < LINES_PER_BLOCK {
            if header.line_is_used(line as u8) {
                header.end = unsafe { lines.add(LINE_SIZE * line) };

                return true;
            }

            line += 1;
        }

        // We reached the end of the block. In this case the hole runs until the
        // end of the block.
        header.end = unsafe { lines.add(BLOCK_SIZE) };

        true
    }

    /// Resets the block to its initial state.
    fn reset(self) {
        let mut header = self.header_mut();

        header.ptr = self.start_address();
        header.end = self.end_address();
        header.previous_block = None;
        header.next_block = None;
        header.reusable_lines.store(0, Ordering::Release);
    }

    /// Rewinds the allocation cursor to the first hole.
    fn find_first_hole(self) {
        self.find_hole_starting_at(FIRST_USABLE_LINE);
    }

    fn previous_block(self) -> Option<BlockPointer> {
        self.header().previous_block
    }

    fn next_block(self) -> Option<BlockPointer> {
        self.header().next_block.as_ref().map(|x| x.as_ptr())
    }

    fn header<'a>(self) -> &'a BlockHeader {
        unsafe { &*(self.ptr.as_ptr() as *mut BlockHeader) }
    }

    fn header_mut<'a>(self) -> &'a mut BlockHeader {
        unsafe { &mut *(self.ptr.as_ptr() as *mut BlockHeader) }
    }

    fn set_next_block(self, block: Block) {
        block.header_mut().previous_block = Some(self);
        self.header_mut().next_block = Some(block);
    }

    fn take_next_block(self) -> Option<Block> {
        self.header_mut().next_block.take().map(|block| {
            block.header_mut().previous_block = None;
            block
        })
    }

    /// Returns a pointer to the start of the first line we can allocate into.
    fn start_address(self) -> *mut u8 {
        unsafe { self.ptr.as_ptr().add(FIRST_ALLOCATION_OFFSET) }
    }

    /// Returns a pointer to the end of the last line we can allocate into.
    fn end_address(self) -> *mut u8 {
        unsafe { self.ptr.as_ptr().add(BLOCK_SIZE) }
    }

    /// Returns a raw pointer to the block.
    fn as_ptr(self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    fn reset_reusable_lines(self) {
        self.header().reusable_lines.store(0, Ordering::Release);
    }

    fn reusable_lines(self) -> u8 {
        self.header().reusable_lines.load(Ordering::Acquire)
    }
}

/// An owned pointer to a block to allocate into.
#[repr(transparent)]
pub struct Block {
    ptr: BlockPointer,
}

impl Block {
    /// Allocates a new block using the system allocator.
    fn new() -> Self {
        let block = unsafe {
            let layout =
                Layout::from_size_align_unchecked(BLOCK_SIZE, BLOCK_SIZE);

            let ptr = alloc_zeroed(layout);

            if ptr.is_null() {
                handle_alloc_error(layout)
            }

            BlockPointer {
                ptr: NonNull::new_unchecked(ptr),
            }
        };

        block.reset();

        Block { ptr: block }
    }

    /// Returns an unowned pointer to the block.
    fn as_ptr(&self) -> BlockPointer {
        self.ptr
    }
}

impl Deref for Block {
    type Target = BlockPointer;

    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

impl DerefMut for Block {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ptr
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        unsafe {
            drop_in_place(self.ptr.as_ptr() as *mut BlockHeader);

            let layout =
                Layout::from_size_align_unchecked(BLOCK_SIZE, BLOCK_SIZE);

            dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

/// Returns an immutable reference to the block header of the given pointer.
///
/// This function assumes the pointer isn't tagged, and is a pointer actually
/// residing on a Block. As no check is in place to enforce this, we've marked
/// this function as unsafe.
unsafe fn block_header_for_pointer<'a>(ptr: *mut u8) -> &'a mut BlockHeader {
    let start = (ptr as isize & BLOCK_START_MASK) as usize;

    &mut *(start as *mut BlockHeader)
}

/// Returns a pointer to the start of the block of the given pointer.
fn line_start_pointer(ptr: *mut u8) -> *mut u8 {
    (ptr as isize & LINE_START_MASK) as _
}

/// Returns the line index for a pointer.
fn line_index_for_pointer(ptr: *mut u8) -> u8 {
    let start = (ptr as isize & BLOCK_START_MASK) as usize;

    ((ptr as usize - start) / LINE_SIZE) as u8
}

/// The bit to set for tagged signed integers.
const INTEGER_BIT: usize = 0;

/// The bit to set for tagged unsigned integers.
///
/// This is the same bit as the reference bit, as integer references are turned
/// into new integers. We use a separate constant name so the purpose is more
/// clear.
const UNSIGNED_INTEGER_BIT: usize = 1;

/// The bit to set for reference pointers.
const REFERENCE_BIT: usize = 1;

/// The bit to set for pointers to permanent objects.
const PERMANENT_BIT: usize = 2;

/// The mask to use for creating tagged unsigned integers.
const UNSIGNED_INTEGER_MASK: usize = 0b11;

/// The mask to use for untagging a pointer.
const UNTAG_MASK: usize = (!0b111) as usize;

/// A pointer to an object managed by the Inko runtime.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Pointer(NonNull<u8>);

unsafe impl Sync for Pointer {}
unsafe impl Send for Pointer {}

impl Pointer {
    /// Creates a new Pointer from the raw address.
    pub unsafe fn new(raw: *mut u8) -> Self {
        Self(NonNull::new_unchecked(raw))
    }

    /// Creates a pointer to a regular boxed value.
    pub fn boxed<T>(value: T) -> Self {
        unsafe { Self::new(Box::into_raw(Box::new(value)) as *mut u8) }
    }

    /// Returns a new Pointer with the given bit set.
    pub unsafe fn with_bit(raw: *mut u8, bit: usize) -> Self {
        Self::new((raw as usize | 1 << bit) as _)
    }

    /// Returns a tagged signed integer.
    pub fn int(value: i64) -> Self {
        unsafe { Self::with_bit((value << 2) as *mut u8, INTEGER_BIT) }
    }

    /// Returns a tagged unsigned integer.
    pub fn unsigned_int(value: u64) -> Self {
        unsafe {
            Self::new(((value << 2) as usize | UNSIGNED_INTEGER_MASK) as _)
        }
    }

    /// Returns a boolean indicating if the pointer is a tagged signed integer.
    pub fn is_tagged_int(self) -> bool {
        self.bit_is_set(INTEGER_BIT)
    }

    /// Returns a boolean indicating if the pointer is a tagged unsigned
    /// integer.
    pub fn is_tagged_unsigned_int(self) -> bool {
        self.bit_is_set(INTEGER_BIT) && self.bit_is_set(UNSIGNED_INTEGER_BIT)
    }

    /// Returns a boolean indicating if the pointer points to a permanent
    /// object.
    pub fn is_permanent(self) -> bool {
        self.bit_is_set(PERMANENT_BIT)
    }

    /// Returns a boolean indicating if the pointer points to a non-permanent
    /// heap object.
    pub fn is_regular_object(self) -> bool {
        (self.as_ptr() as usize & 0b111) == 0
    }

    /// Returns a new pointer with the permanent bit set.
    pub fn as_permanent(self) -> Pointer {
        unsafe { Self::with_bit(self.as_ptr(), PERMANENT_BIT) }
    }

    /// Returns a new pointer with the reference bit set.
    pub fn as_reference(self) -> Pointer {
        unsafe { Self::with_bit(self.as_ptr(), REFERENCE_BIT) }
    }

    /// Returns the raw pointer.
    pub fn as_ptr(self) -> *mut u8 {
        self.0.as_ptr()
    }

    /// Casts the pointer to the given type and returns an immutable reference
    /// to it.
    pub unsafe fn get<'a, T>(self) -> &'a T {
        &*(self.untagged_ptr() as *const T)
    }

    /// Casts the pointer to the given type and returns a mutable reference to
    /// it.
    pub unsafe fn get_mut<'a, T>(self) -> &'a mut T {
        &mut *(self.untagged_ptr() as *mut T)
    }

    /// Drops and deallocates the object this pointer points to.
    ///
    /// This method is meant to be used only when a Pointer points to a value
    /// allocated using Rust's Box type.
    pub unsafe fn drop_boxed<T>(self) {
        drop(Box::from_raw(self.as_ptr() as *mut T));
    }

    /// Returns the given pointer without any tags set.
    pub fn untagged_ptr(self) -> *mut u8 {
        (self.as_ptr() as usize & UNTAG_MASK) as _
    }

    /// Reads the value from a tagged signed integer.
    pub unsafe fn as_int(self) -> i64 {
        self.as_ptr() as i64 >> 2
    }

    /// Reads the value from a tagged unsigned integer.
    pub unsafe fn as_unsigned_int(self) -> u64 {
        self.as_ptr() as u64 >> 2
    }

    /// Marks the underlying memory (based on the given size) as ready for
    /// recycling.
    ///
    /// Recycling is essentially the inverse of an allocation: all lines used by
    /// the object (based on the given size) have their object count reduced.
    ///
    /// Currently this method calculates the lines covered by the object
    /// dynamically, instead of using some sort of cache. We do this because:
    ///
    /// - It should be fast enough (for now)
    /// - It saves us from having to store the line range on a per object basis
    /// - At the time of writing we had no way of measuring if storing this
    ///   range is beneficial and worth the effort
    pub unsafe fn recycle(self, size: usize) {
        let ptr_start = self.as_ptr();
        let ptr_end = (ptr_start as usize + size) as *mut u8;
        let first_line = line_index_for_pointer(ptr_start);
        let mut last_line = line_index_for_pointer(ptr_end);
        let header = block_header_for_pointer(ptr_start);
        let mut reusable = 0;

        if last_line > first_line && ptr_end == line_start_pointer(ptr_end) {
            last_line -= 1;
        }

        for line in first_line..=last_line {
            if header.reduce_line_object_count(line) {
                reusable += 1;
            }
        }

        header.increase_reusable_lines(reusable);
    }

    /// Returns true if the pointer has the given bit set to 1.
    fn bit_is_set(self, bit: usize) -> bool {
        let shifted = 1 << bit;

        (self.as_ptr() as usize & shifted) == shifted
    }
}

/// An allocator used for allocating and handing out blocks of memory.
pub struct BlockAllocator {
    // We use a Vec instead of a linked list. This makes it easier/faster to
    // both reuse existing blocks as well as adding new blocks.
    blocks: Mutex<Vec<Block>>,
}

impl BlockAllocator {
    pub fn new() -> Self {
        Self {
            blocks: Mutex::new(Vec::new()),
        }
    }

    /// Requests a new block.
    pub fn request(&self) -> Block {
        if let Some(block) = self.blocks.lock().pop() {
            block
        } else {
            Block::new()
        }
    }

    /// Adds one or more existing blocks back to the allocator.
    pub fn add_blocks(&self, add: &mut Vec<Block>) {
        self.blocks.lock().append(add);
    }
}

/// A type for allocating objects.
pub trait Allocator {
    /// Allocates a new object of the given size.
    ///
    /// This method expects the size to already be rounded up to a multiple of
    /// 8.
    fn allocate(&mut self, size: usize) -> Pointer;
}

/// An allocator that bump allocates objects into fixed-size blocks.
pub struct BumpAllocator {
    /// The allocator to use for requesting new blocks.
    block_allocator: ArcWithoutWeak<BlockAllocator>,

    /// A linked list of blocks owned by this allocator.
    ///
    /// This is always the first block in the heap, with other blocks linked to
    /// it as a doubly linked list.
    blocks: Block,

    /// The current block we're allocating into.
    ///
    /// This isn't necessarily always the last block in the heap.
    current: BlockPointer,

    /// The last block in the heap.
    ///
    /// This field exists so we can easily move blocks to the end of the heap,
    /// without having to traverse the entire heap.
    last: BlockPointer,

    /// The block to start a recycling scan at.
    recycle_cursor: BlockPointer,

    /// A buffer of empty blocks to return to the global allocator.
    empty_blocks: Vec<Block>,
}

impl BumpAllocator {
    /// Returns a new `BumpAllocator` with the given block as its initial block.
    pub fn new(block_allocator: ArcWithoutWeak<BlockAllocator>) -> Self {
        let block = block_allocator.request();
        let pointer = block.as_ptr();

        Self {
            block_allocator,
            blocks: block,
            current: pointer,
            last: pointer,
            recycle_cursor: pointer,
            empty_blocks: Vec::new(),
        }
    }

    /// Scans a portion of the heap in an attempt to recycle blocks that have
    /// lines we can allocate into.
    ///
    /// Recycling memory is done by traversing a fixed amount of blocks per call
    /// to this method. For every block that has one or more reusable lines, we
    /// move the blocks to the end of the heap. Empty blocks are released back
    /// to the global block allocator.
    ///
    /// When the allocator/mutator resumes, it will skip over lines that are
    /// still in use, and try to fill those that are available.
    ///
    /// This approach won't prevent fragmentation from ever occurring, but it
    /// should help keep it under control; without more expensive techniques as
    /// often employed by garbage collectors (e.g. moving objects around).
    pub fn recycle_blocks(&mut self, config: &Config) {
        let mut pending = config.recycle_blocks_per_scan;

        // Iterates over the blocks, taking ownership of those that can be
        // recycled. In various places we use init!() to overwrite values. This
        // is done so we don't drop the block currently being processed (= the
        // one we want to take ownership of).
        while pending > 0 {
            let block = self.recycle_cursor;
            let reusable = block.reusable_lines();

            pending -= 1;

            if block == self.current {
                // We don't want to clean up the heap beyond the block we're
                // allocating into, as the blocks that follow will have space
                // left anyway. This also ensures that if the heap only has a
                // single block, we don't process it (no point anyway, as we
                // can't move the block around).
                break;
            }

            // Blocks that don't have recyclable lines should be left as-is.
            if reusable == 0 {
                if let Some(next_block) = block.next_block() {
                    self.recycle_cursor = next_block;
                }

                continue;
            }

            if let Some(prev) = block.previous_block() {
                if let Some(next) = block.take_next_block() {
                    // The current block is a block somewhere in the heap, but
                    // not at the start or the end.
                    self.recycle_cursor = next.as_ptr();

                    init!(prev.header_mut().next_block => Some(next));
                }
            } else if let Some(next) = block.take_next_block() {
                // The current block is the first block in the heap, and
                // more blocks follow it.
                self.recycle_cursor = next.as_ptr();

                init!(self.blocks => next);
            }

            block.reset_reusable_lines();

            // At this point we have taken the block out of the list, and we can
            // safely take ownership of it.
            let owned_block = Block { ptr: block };

            if reusable == USABLE_LINES_PER_BLOCK {
                owned_block.reset();
                self.empty_blocks.push(owned_block);
            } else {
                self.recycle_block(owned_block);
            }
        }

        if !self.empty_blocks.is_empty() {
            self.block_allocator.add_blocks(&mut self.empty_blocks);
        }
    }

    fn add_new_block(&mut self) {
        let block = self.block_allocator.request();
        let ptr = block.as_ptr();

        self.current.set_next_block(block);

        self.current = ptr;
        self.last = ptr;
    }

    fn recycle_block(&mut self, block: Block) {
        let block_ptr = block.as_ptr();

        // We find the first hole here so the allocator doesn't have to spend
        // extra cycles doing so.
        block.find_first_hole();
        self.last.set_next_block(block);

        self.last = block_ptr;
    }
}

impl Allocator for BumpAllocator {
    fn allocate(&mut self, size: usize) -> Pointer {
        loop {
            if let Some(ptr) = self.current.allocate(size) {
                return unsafe { Pointer::new(ptr.as_ptr()) };
            }

            if self.current.find_hole() {
                continue;
            }

            if let Some(next) = self.current.next_block() {
                self.current = next;

                continue;
            }

            self.add_new_block();
        }
    }
}

/// An allocator for permanent objects.
pub struct PermanentAllocator {
    allocator: BumpAllocator,
}

impl PermanentAllocator {
    /// Returns a new permanent allocator with a single reserved block.
    pub fn new(block_allocator: ArcWithoutWeak<BlockAllocator>) -> Self {
        PermanentAllocator {
            allocator: BumpAllocator::new(block_allocator),
        }
    }
}

impl Allocator for PermanentAllocator {
    fn allocate(&mut self, size: usize) -> Pointer {
        self.allocator.allocate(size).as_permanent()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    fn block_allocator() -> ArcWithoutWeak<BlockAllocator> {
        ArcWithoutWeak::new(BlockAllocator::new())
    }

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<BumpAllocator>(), 64);
        assert_eq!(size_of::<BlockAllocator>(), 32);
        assert_eq!(size_of::<BlockHeader>(), 296);
        assert_eq!(size_of::<Block>(), 8);
        assert_eq!(size_of::<Option<Block>>(), 8);
    }

    #[test]
    fn test_block_header_for_pointer() {
        let block = Block::new();
        let ptr = unsafe {
            block.ptr.as_ptr().add(LINE_SIZE + FIRST_ALLOCATION_OFFSET)
        };
        let header_ref = unsafe { block_header_for_pointer(ptr) };

        assert!(header_ref.end == block.header().end);
    }

    #[test]
    fn test_line_index_for_pointer() {
        let block = Block::new();

        unsafe {
            for i in 0..128 {
                assert_eq!(
                    line_index_for_pointer(block.ptr.as_ptr().add(i)),
                    0
                );
            }

            for i in 128..256 {
                assert_eq!(
                    line_index_for_pointer(block.ptr.as_ptr().add(i)),
                    1
                );
            }

            for i in 256..384 {
                assert_eq!(
                    line_index_for_pointer(block.ptr.as_ptr().add(i)),
                    2
                );
            }

            assert_eq!(
                line_index_for_pointer(block.ptr.as_ptr().add(BLOCK_SIZE - 1))
                    as usize,
                LINES_PER_BLOCK - 1
            );
        }
    }

    #[test]
    fn test_block_allocate() {
        let block = Block::new();

        assert!(block.allocate(8).is_some());
        assert!(block.header().ptr == unsafe { block.start_address().add(8) });
        assert!(block.allocate(BLOCK_SIZE - LINE_SIZE).is_none());
        assert_eq!(block.header().line_object_count(3), 1);
    }

    #[test]
    fn test_block_allocate_line_overflow() {
        let block = Block::new();

        block.allocate(127);
        block.allocate(130);

        assert_eq!(block.header().line_object_count(3), 2);
        assert_eq!(block.header().line_object_count(4), 1);
        assert_eq!(block.header().line_object_count(5), 1);
    }

    #[test]
    fn test_block_find_hole() {
        let block = Block::new();
        let ptr = block.ptr;
        let header = block.header();

        assert!(block.find_hole());

        unsafe {
            assert_eq!(
                header.ptr,
                ptr.as_ptr().add(FIRST_ALLOCATION_OFFSET + LINE_SIZE)
            );

            assert_eq!(header.end, ptr.as_ptr().add(BLOCK_SIZE));
        }
    }

    #[test]
    fn test_block_find_hole_between_two_used_lines() {
        let block = Block::new();

        {
            let header = block.header_mut();

            header.increase_line_object_count(4);
            header.increase_line_object_count(6);
        }

        assert!(block.find_hole());

        let ptr = block.ptr;
        let header = block.header();

        unsafe {
            assert_eq!(header.ptr, ptr.as_ptr().add(LINE_SIZE * 5));
            assert_eq!(header.end, ptr.as_ptr().add(LINE_SIZE * 6));
        }
    }

    #[test]
    fn test_block_find_hole_full_block() {
        let block = Block::new();

        {
            let header = block.header_mut();

            for i in 0..header.line_map.len() {
                header.increase_line_object_count(i as u8);
            }
        }

        assert_eq!(block.find_hole(), false);

        let ptr = block.ptr;
        let header = block.header();

        unsafe {
            assert_eq!(header.ptr, ptr.as_ptr().add(FIRST_ALLOCATION_OFFSET));
            assert_eq!(header.end, ptr.as_ptr().add(BLOCK_SIZE));
        }
    }

    #[test]
    fn test_block_find_hole_hole_starts_at_end() {
        let block = Block::new();

        block.header_mut().ptr = block.end_address();

        assert_eq!(block.find_hole(), false);
    }

    #[test]
    fn test_block_find_hole_starting_at() {
        let block = Block::new();

        {
            let header = block.header_mut();

            header.increase_line_object_count(3);
            header.increase_line_object_count(5);
        }

        assert!(block.find_hole_starting_at(FIRST_USABLE_LINE));

        let ptr = block.ptr;
        let header = block.header();

        assert_eq!(header.ptr, unsafe { ptr.as_ptr().add(LINE_SIZE * 4) });
        assert_eq!(header.end, unsafe { ptr.as_ptr().add(LINE_SIZE * 5) });
    }

    #[test]
    fn test_block_find_hole_starting_at_with_trailing_empty_lines() {
        let block = Block::new();

        {
            let header = block.header_mut();

            header.increase_line_object_count(3);
        }

        assert!(block.find_hole_starting_at(FIRST_USABLE_LINE));

        let ptr = block.ptr;
        let header = block.header();

        assert!(header.ptr == unsafe { ptr.as_ptr().add(LINE_SIZE * 4) });
        assert!(header.end == unsafe { ptr.as_ptr().add(BLOCK_SIZE) });
    }

    #[test]
    fn test_block_set_next_block() {
        let block1 = Block::new();
        let block2 = Block::new();
        let block2_ptr = block2.as_ptr();

        block1.set_next_block(block2);

        let next = block1.next_block().unwrap();

        assert_eq!(next, block2_ptr);
        assert_eq!(next.header().previous_block, Some(block1.as_ptr()));
        assert_eq!(next.next_block(), None);
    }

    #[test]
    fn test_block_take_next_block() {
        let block1 = Block::new();
        let block2 = Block::new();
        let block2_ptr = block2.as_ptr();

        block1.set_next_block(block2);

        let next = block1.take_next_block().unwrap();

        assert!(block1.next_block().is_none());
        assert!(block1.header().previous_block.is_none());
        assert!(next.next_block().is_none());
        assert!(next.header().previous_block.is_none());
        assert_eq!(next.as_ptr(), block2_ptr);
    }

    #[test]
    fn test_block_allocator_request_and_add() {
        let alloc = block_allocator();
        let block = alloc.request();

        assert_eq!(alloc.blocks.lock().len(), 0);

        alloc.add_blocks(&mut vec![block]);

        assert_eq!(alloc.blocks.lock().len(), 1);
    }

    #[test]
    fn test_bump_allocator_allocate_in_hole() {
        let blocks = block_allocator();
        let mut bump = BumpAllocator::new(blocks);
        let ptr = bump.allocate(8);

        assert_eq!(unsafe { *(ptr.as_ptr() as *mut u64) }, 0);
        assert_eq!(line_index_for_pointer(ptr.as_ptr()), 3);
    }

    #[test]
    fn test_bump_allocator_find_hole() {
        let blocks = block_allocator();
        let mut bump = BumpAllocator::new(blocks);
        let block = bump.current;

        block.header_mut().increase_line_object_count(4);
        block.find_hole_starting_at(FIRST_USABLE_LINE);

        let ptr1 = bump.allocate(127);
        let ptr2 = bump.allocate(8);

        assert_eq!(line_index_for_pointer(ptr1.as_ptr()), 3);
        assert_eq!(line_index_for_pointer(ptr2.as_ptr()), 5);
    }

    #[test]
    fn test_bump_allocator_find_next_block() {
        let blocks = block_allocator();
        let mut bump = BumpAllocator::new(blocks);

        let obj1 = bump.allocate(128);

        for _ in 0..(LINES_PER_BLOCK - (FIRST_USABLE_LINE + 1) as usize) {
            bump.allocate(128);
        }

        let obj2 = bump.allocate(8);

        assert_eq!(line_index_for_pointer(obj1.as_ptr()), 3);
        assert_eq!(line_index_for_pointer(obj2.as_ptr()), 3);
        assert_eq!(bump.blocks.next_block(), Some(bump.current));
        assert_ne!(obj1.as_ptr(), obj2.as_ptr());
    }

    #[test]
    #[should_panic]
    fn test_bump_allocator_too_large() {
        let mut bump = BumpAllocator::new(block_allocator());

        bump.allocate(1 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_bump_allocator_add_new_block() {
        let mut bump = BumpAllocator::new(block_allocator());

        bump.add_new_block();

        let next = bump.blocks.next_block().unwrap();

        assert_eq!(next, bump.current);
        assert_eq!(next.next_block(), None);
        assert_eq!(next.previous_block(), Some(bump.blocks.as_ptr()));
        assert_eq!(bump.current, bump.last);
    }

    #[test]
    fn test_bump_allocator_recycle_block() {
        let mut bump = BumpAllocator::new(block_allocator());
        let block = Block::new();
        let block_ptr = block.as_ptr();
        let mut header = block.header_mut();

        header.ptr = header.end;

        bump.recycle_block(block);

        assert_eq!(bump.current, bump.blocks.as_ptr());
        assert_eq!(bump.current.next_block(), Some(block_ptr));
        assert_eq!(bump.last, block_ptr);
        assert_eq!(header.ptr, block_ptr.start_address());
        assert_eq!(block_ptr.next_block(), None);
        assert_eq!(block_ptr.previous_block(), Some(bump.blocks.as_ptr()));
    }

    #[test]
    fn test_bump_allocator_recycle_blocks_with_single_empty_block() {
        let mut bump = BumpAllocator::new(block_allocator());
        let config = Config::new();

        bump.blocks.header_mut().ptr = bump.blocks.end_address();

        bump.recycle_blocks(&config);

        assert_eq!(bump.blocks.header().ptr, bump.blocks.end_address());
        assert_eq!(bump.blocks.reusable_lines(), 0);
        assert_eq!(bump.block_allocator.blocks.lock().len(), 0);
        assert_eq!(bump.recycle_cursor, bump.blocks.as_ptr());
    }

    #[test]
    fn test_bump_allocator_recycle_blocks_with_single_reusable_block() {
        let mut bump = BumpAllocator::new(block_allocator());
        let config = Config::new();

        bump.blocks.header_mut().ptr = bump.blocks.end_address();

        bump.blocks.header_mut().increase_reusable_lines(1);
        bump.recycle_blocks(&config);

        assert_eq!(bump.blocks.header().ptr, bump.blocks.end_address());
        assert_eq!(bump.blocks.reusable_lines(), 1);
        assert_eq!(bump.recycle_cursor, bump.blocks.as_ptr());
        assert_eq!(bump.last, bump.blocks.as_ptr());
    }

    #[test]
    fn test_bump_allocator_recycle_blocks_with_only_empty_blocks() {
        let mut bump = BumpAllocator::new(block_allocator());
        let config = Config::new();
        let block1 = bump.current;

        bump.add_new_block();

        let block2 = bump.current;

        bump.recycle_blocks(&config);

        assert_eq!(bump.current, block2);
        assert_eq!(bump.current.next_block(), None);
        assert_eq!(bump.recycle_cursor, block2);
        assert_eq!(bump.block_allocator.blocks.lock().len(), 0);
        assert_eq!(bump.last, block2);
        assert_eq!(bump.blocks.as_ptr(), block1);
        assert_eq!(block1.next_block(), Some(block2));
    }

    #[test]
    fn test_bump_allocator_recycle_blocks_skip_blocks_after_current() {
        let mut bump = BumpAllocator::new(block_allocator());
        let config = Config::new();
        let block1 = bump.current;

        bump.add_new_block();

        let block2 = bump.current;

        bump.current = block1;

        block1.header_mut().increase_reusable_lines(1);
        block2.header_mut().increase_reusable_lines(1);
        bump.recycle_blocks(&config);

        assert_eq!(bump.current, block1);
        assert_eq!(bump.last, block2);
        assert_eq!(bump.recycle_cursor, block1);
        assert_eq!(block1.reusable_lines(), 1);
        assert_eq!(block2.reusable_lines(), 1);
        assert_eq!(bump.block_allocator.blocks.lock().len(), 0);
    }

    #[test]
    fn test_bump_allocator_recycle_blocks_with_fully_reusable_blocks() {
        let mut bump = BumpAllocator::new(block_allocator());
        let config = Config::new();
        let block1 = bump.current;

        bump.add_new_block();

        let block2 = bump.current;
        let max = USABLE_LINES_PER_BLOCK;

        block2.header_mut().ptr = block2.end_address();

        block1.header_mut().increase_reusable_lines(max);
        block2.header_mut().increase_reusable_lines(max);
        bump.recycle_blocks(&config);

        assert_eq!(bump.current, block2);
        assert_eq!(bump.last, block2);
        assert_eq!(bump.recycle_cursor, block2);
        assert_eq!(block1.reusable_lines(), 0);
        assert_eq!(block2.reusable_lines(), max);
        assert_eq!(block1.header().ptr, block1.start_address());
        assert_eq!(block2.header().ptr, block2.end_address());
        assert_eq!(bump.block_allocator.blocks.lock().len(), 1);

        let released = bump
            .block_allocator
            .blocks
            .lock()
            .last()
            .map(|x| x.as_ptr())
            .unwrap();

        assert_eq!(released, block1);
    }

    #[test]
    fn test_bump_allocator_recycle_blocks_with_reusable_first_block() {
        let mut bump = BumpAllocator::new(block_allocator());
        let config = Config::new();
        let block1 = bump.current;

        bump.add_new_block();

        let block2 = bump.current;

        block1.header_mut().increase_reusable_lines(1);
        bump.recycle_blocks(&config);

        assert_eq!(bump.current, block2);
        assert_eq!(bump.last, block1);
        assert_eq!(bump.recycle_cursor, block2);
        assert_eq!(block1.reusable_lines(), 0);
        assert_eq!(bump.block_allocator.blocks.lock().len(), 0);
        assert_eq!(bump.blocks.as_ptr(), block2);
    }

    #[test]
    fn test_bump_allocator_recycle_blocks_with_reusable_middle_block() {
        let mut bump = BumpAllocator::new(block_allocator());
        let config = Config::new();
        let block1 = bump.current;

        bump.add_new_block();

        let block2 = bump.current;

        bump.add_new_block();

        let block3 = bump.current;

        block2.header_mut().increase_reusable_lines(1);
        bump.recycle_blocks(&config);

        assert_eq!(bump.current, block3);
        assert_eq!(bump.last, block2);
        assert_eq!(bump.recycle_cursor, block3);
        assert_eq!(block2.reusable_lines(), 0);
        assert_eq!(bump.block_allocator.blocks.lock().len(), 0);
        assert_eq!(bump.blocks.as_ptr(), block1);
    }

    #[test]
    fn test_permanent_allocator_allocate() {
        let mut alloc = PermanentAllocator::new(block_allocator());
        let ptr = alloc.allocate(8);

        assert!(ptr.is_permanent());
    }

    #[test]
    fn test_pointer_with_bit() {
        let ptr = unsafe { Pointer::with_bit(0x4 as _, 1) };

        assert_eq!(ptr.0.as_ptr() as usize, 0x6);
    }

    #[test]
    fn test_pointer_integer_tagging() {
        unsafe {
            assert_eq!(Pointer::int(3).as_int(), 3);
            assert_eq!(Pointer::int(0).as_int(), 0);
            assert_eq!(Pointer::int(-3).as_int(), -3);

            assert!(Pointer::int(3).is_tagged_int());
        }
    }

    #[test]
    fn test_pointer_unsigned_integer_tagging() {
        unsafe {
            assert_eq!(Pointer::unsigned_int(0).as_unsigned_int(), 0);
            assert_eq!(Pointer::unsigned_int(3).as_unsigned_int(), 3);

            assert!(Pointer::unsigned_int(0).is_tagged_unsigned_int());
        }
    }

    #[test]
    fn test_pointer_is_permanent_object() {
        unsafe {
            assert!(!Pointer::new(0x0 as _).is_permanent());
            assert!(Pointer::new(0x0 as _).as_permanent().is_permanent());
        }
    }

    #[test]
    fn test_pointer_as_reference() {
        assert_eq!(
            unsafe { Pointer::new(0x1 as _) }.as_reference().0.as_ptr()
                as usize,
            0x3
        );
    }

    #[test]
    fn test_pointer_is_regular_object() {
        let mut perm = PermanentAllocator::new(block_allocator());
        let mut bump = BumpAllocator::new(block_allocator());

        assert!(!perm.allocate(8).is_regular_object());
        assert!(!Pointer::int(42).is_regular_object());
        assert!(!Pointer::unsigned_int(42).is_regular_object());
        assert!(!bump.allocate(8).as_reference().is_regular_object());
        assert!(bump.allocate(8).is_regular_object());
    }

    #[test]
    fn test_pointer_get() {
        let mut bump = BumpAllocator::new(block_allocator());
        let ptr = bump.allocate(size_of::<u64>());

        unsafe {
            assert_eq!(*ptr.get::<u64>(), 0);
        }
    }

    #[test]
    fn test_pointer_get_mut() {
        let mut bump = BumpAllocator::new(block_allocator());
        let ptr = bump.allocate(size_of::<u64>());

        unsafe {
            let val = ptr.get_mut::<u64>();

            *val = 42;

            assert_eq!(*val, 42);
        }
    }

    #[test]
    fn test_pointer_drop_boxed() {
        unsafe {
            let ptr = Pointer::boxed(42_u64);

            // This is just a smoke test to make sure dropping a Box doesn't
            // crash or leak.
            ptr.drop_boxed::<u64>();
        }
    }

    #[test]
    fn test_pointer_untagged_ptr() {
        assert_eq!(
            unsafe { Pointer::new(0x8 as _) }
                .as_reference()
                .untagged_ptr(),
            0x8 as *mut u8
        );
    }

    #[test]
    fn test_pointer_recycle_single_line_pointer() {
        let mut bump = BumpAllocator::new(block_allocator());
        let size = 64;
        let ptr = bump.allocate(size);
        let block = bump.current;
        let line = FIRST_USABLE_LINE;

        assert_eq!(block.header().line_object_count(line), 1);
        assert_eq!(block.reusable_lines(), 0);

        unsafe { ptr.recycle(size) };

        assert_eq!(block.header().line_object_count(line), 0);
        assert_eq!(block.reusable_lines(), 1);
    }

    #[test]
    fn test_pointer_recycle_multi_line_pointer() {
        let mut bump = BumpAllocator::new(block_allocator());
        let size = 256;
        let ptr = bump.allocate(size);
        let block = bump.current;
        let line1 = FIRST_USABLE_LINE;
        let line2 = line1 + 1;

        assert_eq!(block.header().line_object_count(line1), 1);
        assert_eq!(block.header().line_object_count(line2), 1);
        assert_eq!(block.reusable_lines(), 0);

        unsafe { ptr.recycle(size) };

        assert_eq!(block.header().line_object_count(line1), 0);
        assert_eq!(block.header().line_object_count(line2), 0);
        assert_eq!(block.reusable_lines(), 2);
    }

    #[test]
    fn test_pointer_recycle_overflowing_pointer() {
        let mut bump = BumpAllocator::new(block_allocator());
        let size = 129;
        let ptr = bump.allocate(size);
        let block = bump.current;
        let line1 = FIRST_USABLE_LINE;
        let line2 = line1 + 1;

        assert_eq!(block.header().line_object_count(line1), 1);
        assert_eq!(block.header().line_object_count(line2), 1);
        assert_eq!(block.reusable_lines(), 0);

        unsafe { ptr.recycle(size) };

        assert_eq!(block.header().line_object_count(line1), 0);
        assert_eq!(block.header().line_object_count(line2), 0);
        assert_eq!(block.reusable_lines(), 2);
    }
}
