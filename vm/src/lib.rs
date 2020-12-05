#![cfg_attr(feature = "cargo-clippy", allow(renamed_and_removed_lints))]
#![cfg_attr(feature = "cargo-clippy", allow(new_without_default))]
#![cfg_attr(feature = "cargo-clippy", allow(needless_range_loop))]
#![cfg_attr(feature = "cargo-clippy", allow(missing_safety_doc))]

pub mod arc_without_weak;
pub mod binding;
pub mod block;
pub mod broadcast;
pub mod bytecode_parser;
pub mod catch_table;
pub mod chunk;
pub mod closable;
pub mod compiled_code;
pub mod config;
pub mod date_time;
pub mod deref_pointer;
pub mod directories;
pub mod duration;
pub mod error_messages;
pub mod execution_context;
pub mod ffi;
pub mod file;
pub mod filesystem;
pub mod gc;
pub mod generator;
pub mod global_scope;
pub mod hasher;
pub mod immix;
pub mod immutable_string;
pub mod integer_operations;
pub mod macros;
pub mod mailbox;
pub mod module;
pub mod modules;
pub mod network_poller;
pub mod numeric;
pub mod object;
pub mod object_pointer;
pub mod object_value;
pub mod objects;
pub mod platform;
pub mod process;
pub mod process_status;
pub mod registers;
pub mod runtime_error;
pub mod runtime_panic;
pub mod scheduler;
pub mod slicing;
pub mod socket;
pub mod string_pool;
pub mod tagged_pointer;
pub mod vm;
