//!  Inko bytecode interpreter.
use crate::image::Image;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Method, String as InkoString};
use crate::mem::process::ServerPointer;
use crate::network_poller::Worker as NetworkPollerWorker;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::instruction::Instruction;
use crate::vm::instruction::Opcode;
use crate::vm::instructions::array;
use crate::vm::instructions::byte_array;
use crate::vm::instructions::external_functions;
use crate::vm::instructions::float;
use crate::vm::instructions::future;
use crate::vm::instructions::general;
use crate::vm::instructions::generator;
use crate::vm::instructions::integer;
use crate::vm::instructions::module;
use crate::vm::instructions::object;
use crate::vm::instructions::process;
use crate::vm::instructions::string;
use crate::vm::state::{RcState, State};
use std::thread;

/// The number of reductions to apply for a method call.
const METHOD_REDUCTION_COST: usize = 1;

macro_rules! throw_value {
    ($machine: expr, $generator: expr, $value: expr) => {{
        $generator.set_throw_value($value);

        if $generator.pop_context() {
            return Err("Can't throw from the root generator".to_string());
        }

        continue;
    }};
}

macro_rules! throw_error_message {
    ($machine: expr, $worker: expr, $generator: expr, $message: expr) => {{
        let value = InkoString::alloc(
            $worker.allocator(),
            $machine.state.permanent_space.string_class(),
            $message,
        );

        throw_value!($machine, $generator, value);
    }};
}

macro_rules! reduce {
    ($vm:expr, $process:expr, $reductions:expr) => {{
        let reduce_by = METHOD_REDUCTION_COST;

        if $reductions >= reduce_by {
            $reductions -= reduce_by;
        } else {
            $vm.state.scheduler.schedule($process);
            return Ok(());
        }
    }};
}

/// Handles an operation that may produce IO errors.
macro_rules! try_io_error {
    ($expr:expr, $vm:expr, $worker:expr, $gen:expr) => {{
        // When an operation would block, the socket is already registered, and
        // the process may already be running again in another thread. This
        // means that when a WouldBlock is produced it is not safe to access any
        // process data.
        //
        // To ensure blocking operations are retried properly, we _first_ set
        // the instruction index, then advance it again if it is safe to do so.
        $gen.context.index -= 1;

        match $expr {
            Ok(thing) => {
                $gen.context.index += 1;

                thing
            }
            Err(RuntimeError::Panic(msg)) => {
                vm_panic!(msg, $gen);
            }
            Err(RuntimeError::ErrorMessage(msg)) => {
                throw_error_message!($vm, $worker, $gen, msg);
            }
            Err(RuntimeError::Error(err)) => {
                throw_value!($vm, $gen, err);
            }
            Err(RuntimeError::WouldBlock) => {
                // *DO NOT* use "$gen" at this point, as it may have been
                // invalidated if the process is already running again in
                // another thread.
                return Ok(());
            }
        }
    }};
}

/// Handles a regular runtime error.
macro_rules! try_error {
    ($expr: expr, $vm: expr, $worker: expr, $gen: expr) => {{
        match $expr {
            Ok(thing) => thing,
            Err(RuntimeError::Panic(msg)) => {
                vm_panic!(msg, $gen);
            }
            Err(RuntimeError::ErrorMessage(msg)) => {
                throw_error_message!($vm, $worker, $gen, msg);
            }
            Err(RuntimeError::Error(err)) => {
                throw_value!($vm, $gen, err);
            }
            _ => unreachable!(),
        }
    }};
}

macro_rules! vm_panic {
    ($message: expr, $gen: expr) => {{
        // We subtract one so the instruction pointer points to the current
        // instruction (= the panic), not the one we'd run after that.
        $gen.context.index -= 1;

        return Err($message);
    }};
}

pub struct Machine<'a> {
    /// The shared virtual machine state, such as the process pools and built-in
    /// types.
    pub state: &'a State,
}

impl<'a> Machine<'a> {
    pub fn new(state: &'a State) -> Self {
        Machine { state }
    }

    /// Boots up the VM and all its thread pools.
    ///
    /// This method blocks the calling thread until the Inko program terminates.
    pub fn boot(image: Image, arguments: &[String]) -> Result<RcState, String> {
        let state = State::new(image.config, image.permanent_space, arguments);
        let entry_module =
            state.permanent_space.get_module(&image.entry_module)?;

        let entry_method = Method::lookup(
            &state.permanent_space,
            entry_module.as_pointer(),
            image.entry_method,
        );

        let secondary_guard =
            state.scheduler.blocking_pool.start(state.clone());

        let timeout_guard = {
            let thread_state = state.clone();

            thread::Builder::new()
                .name("timeout worker".to_string())
                .spawn(move || {
                    thread_state.timeout_worker.run(&thread_state.scheduler);
                })
                .unwrap()
        };

        let poller_guard = {
            let thread_state = state.clone();

            thread::Builder::new()
                .name("network poller".to_string())
                .spawn(move || {
                    NetworkPollerWorker::new(thread_state).run();
                })
                .unwrap()
        };

        // Starting the primary threads will block this thread, as the main
        // worker will run directly onto the current thread. As such, we must
        // start these threads last.
        let primary_guard = {
            let thread_state = state.clone();

            state
                .scheduler
                .primary_pool
                .start_main(thread_state, entry_method)
        };

        // Joining the pools only fails in case of a panic. In this case we
        // don't want to re-panic as this clutters the error output.
        if primary_guard.join().is_err()
            || secondary_guard.join().is_err()
            || timeout_guard.join().is_err()
            || poller_guard.join().is_err()
        {
            state.set_exit_status(1);
        }

        Ok(state)
    }

    pub fn run(&self, worker: &mut ProcessWorker, mut process: ServerPointer) {
        let gen = process.generator_to_run(
            worker.allocator(),
            self.state.permanent_space.generator_class(),
        );

        // When there's no generator to run, clients will try to reschedule the
        // process after sending it a message. This means we (here) don't need
        // to do anything extra.
        if let Some(gen) = gen {
            if let Err(message) = self.run_generator(worker, process, gen) {
                self.panic(process, gen, &message);
            }
        } else {
            process::finish_generator(&self.state, worker, process);
        }
    }

    fn run_generator(
        &self,
        worker: &mut ProcessWorker,
        mut process: ServerPointer,
        mut generator: GeneratorPointer,
    ) -> Result<(), String> {
        let mut reductions = self.state.config.reductions;

        'exec_loop: loop {
            let instruction = unsafe {
                let idx = generator.context.index;

                // We need a reference to the instruction, but we also need to
                // borrow the generator in various places. This is fine because
                // instructions are stored external to a generator and its
                // context, but Rust doesn't know this.
                //
                // To work around this, we get a reference to the instruction,
                // turn it into a pointer, then turn it back into a reference.
                // This way Rust's borrow checker loses track of it, and we can
                // do whatever we want (with all the risks of doing so of
                // course).
                //
                // Perhaps one day there is a better way of doing this.
                &*(generator.context.method.instruction(idx)
                    as *const Instruction)
            };

            generator.context.index += 1;

            match instruction.opcode {
                Opcode::GetLiteral => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::get_literal(generator, idx);

                    generator.context.set_register(reg, res);
                }
                Opcode::GetLiteralWide => {
                    let reg = instruction.arg(0);
                    let arg1 = instruction.arg(1);
                    let arg2 = instruction.arg(2);
                    let res = general::get_literal_wide(generator, arg1, arg2);

                    generator.context.set_register(reg, res);
                }
                Opcode::Allocate => {
                    let reg = instruction.arg(0);
                    let class =
                        generator.context.get_register(instruction.arg(1));
                    let res = object::allocate(worker, class);

                    generator.context.set_register(reg, res);
                }
                Opcode::ArrayAllocate => {
                    let reg = instruction.arg(0);
                    let start = instruction.arg(1);
                    let len = instruction.arg(2);
                    let res = array::allocate(
                        &self.state,
                        worker,
                        generator,
                        start,
                        len,
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::GetTrue => {
                    let res = self.state.permanent_space.true_singleton;

                    generator.context.set_register(instruction.arg(0), res);
                }
                Opcode::GetFalse => {
                    let res = self.state.permanent_space.false_singleton;

                    generator.context.set_register(instruction.arg(0), res);
                }
                Opcode::SetLocal => {
                    let idx = instruction.arg(0);
                    let val =
                        generator.context.get_register(instruction.arg(1));

                    general::set_local(generator, idx, val);
                }
                Opcode::GetLocal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::get_local(generator, idx);

                    generator.context.set_register(reg, res);
                }
                Opcode::Return => {
                    let res =
                        generator.context.get_register(instruction.arg(0));

                    generator.set_result(res);

                    // Once we're at the top-level _and_ we have no more
                    // instructions to process, we'll write the result to a
                    // future and bail out the execution loop.
                    if generator.pop_context() {
                        generator.finish();

                        break 'exec_loop;
                    }

                    reduce!(self, process, reductions);
                }
                Opcode::GotoIfFalse => {
                    let val =
                        generator.context.get_register(instruction.arg(1));

                    if val == self.state.permanent_space.false_singleton {
                        generator.context.index = instruction.arg(0) as usize;
                    }
                }
                Opcode::GotoIfTrue => {
                    let val =
                        generator.context.get_register(instruction.arg(1));

                    if val == self.state.permanent_space.true_singleton {
                        generator.context.index = instruction.arg(0) as usize;
                    }
                }
                Opcode::Goto => {
                    generator.context.index = instruction.arg(0) as usize;
                }
                Opcode::GotoIfThrown => {
                    if generator.thrown() {
                        generator.context.index = instruction.arg(0) as usize;
                    }
                }
                Opcode::IntAdd => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_add(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntDiv => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = try_error!(
                        integer::int_div(&self.state, worker, a, b),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::IntMul => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_mul(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntSub => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_sub(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntMod => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_modulo(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntBitwiseAnd => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_and(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntBitwiseOr => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_or(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntBitwiseXor => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_xor(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntShiftLeft => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_shl(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntShiftRight => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_shr(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntSmaller => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_smaller(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntGreater => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_greater(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntEquals => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_equals(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntGreaterOrEqual => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_greater_or_equal(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::IntSmallerOrEqual => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::int_smaller_or_equal(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntAdd => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_add(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntDiv => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = try_error!(
                        integer::unsigned_int_div(&self.state, worker, a, b),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntMul => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_mul(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntSub => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_sub(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntMod => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_modulo(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntBitwiseAnd => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_and(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntBitwiseOr => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_or(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntBitwiseXor => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_xor(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntShiftLeft => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_shl(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntShiftRight => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res =
                        integer::unsigned_int_shr(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntSmaller => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::unsigned_int_smaller(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntGreater => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::unsigned_int_greater(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntEquals => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::unsigned_int_equals(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntGreaterOrEqual => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::unsigned_int_greater_or_equal(
                        &self.state,
                        a,
                        b,
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::UnsignedIntSmallerOrEqual => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = integer::unsigned_int_smaller_or_equal(
                        &self.state,
                        a,
                        b,
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatAdd => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::add(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatMul => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::mul(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatDiv => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::div(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatSub => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::sub(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatMod => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::modulo(&self.state, worker, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatSmaller => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::smaller(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatGreater => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::greater(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatEquals => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::equals(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatGreaterOrEqual => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::greater_or_equal(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::FloatSmallerOrEqual => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = float::smaller_or_equal(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::ArraySet => {
                    let ary =
                        generator.context.get_register(instruction.arg(0));
                    let idx =
                        generator.context.get_register(instruction.arg(1));
                    let val =
                        generator.context.get_register(instruction.arg(2));

                    try_error!(
                        array::set(ary, idx, val),
                        self,
                        worker,
                        generator
                    );
                }
                Opcode::ArrayGet => {
                    let reg = instruction.arg(0);
                    let ary =
                        generator.context.get_register(instruction.arg(1));
                    let idx =
                        generator.context.get_register(instruction.arg(2));
                    let res = try_error!(
                        array::get(ary, idx),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ArrayRemove => {
                    let reg = instruction.arg(0);
                    let ary =
                        generator.context.get_register(instruction.arg(1));
                    let idx =
                        generator.context.get_register(instruction.arg(2));
                    let res = try_error!(
                        array::remove(ary, idx),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ArrayLength => {
                    let reg = instruction.arg(0);
                    let ary =
                        generator.context.get_register(instruction.arg(1));
                    let res = array::length(&self.state, worker, ary);

                    generator.context.set_register(reg, res);
                }
                Opcode::StringEquals => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = string::equals(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::StringLength => {
                    let reg = instruction.arg(0);
                    let val =
                        generator.context.get_register(instruction.arg(1));
                    let res = string::length(&self.state, worker, val);

                    generator.context.set_register(reg, res);
                }
                Opcode::StringSize => {
                    let reg = instruction.arg(0);
                    let val =
                        generator.context.get_register(instruction.arg(1));
                    let res = string::size(&self.state, worker, val);

                    generator.context.set_register(reg, res);
                }
                Opcode::LoadModule => {
                    let mod_reg = instruction.arg(0);
                    let exe_reg = instruction.arg(1);
                    let name =
                        generator.context.get_register(instruction.arg(2));

                    let (module, exe) = module::load(&self.state, name)?;

                    generator.context.set_register(mod_reg, module);
                    generator.context.set_register(exe_reg, exe);
                }
                Opcode::GetCurrentModule => {
                    let reg = instruction.arg(0);
                    let res = module::current(generator);

                    generator.context.set_register(reg, res);
                }
                Opcode::SetField => {
                    let rec =
                        generator.context.get_register(instruction.arg(0));
                    let idx = instruction.arg(1);
                    let val =
                        generator.context.get_register(instruction.arg(2));

                    object::set_field(rec, idx, val);
                }
                Opcode::GetField => {
                    let reg = instruction.arg(0);
                    let rec =
                        generator.context.get_register(instruction.arg(1));
                    let idx = instruction.arg(2);
                    let res = object::get_field(rec, idx);

                    generator.context.set_register(reg, res);
                }
                Opcode::GetClass => {
                    let reg = instruction.arg(0);
                    let src =
                        generator.context.get_register(instruction.arg(1));
                    let res = object::get_class(&self.state, src);

                    generator.context.set_register(reg, res);
                }
                Opcode::ProcessAllocate => {
                    let reg = instruction.arg(0);
                    let class =
                        generator.context.get_register(instruction.arg(1));
                    let start = instruction.arg(2);
                    let args = instruction.arg(3);
                    let res = process::allocate(
                        &self.state,
                        worker,
                        generator,
                        class,
                        start,
                        args,
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ProcessSendMessage => {
                    let client =
                        generator.context.get_register(instruction.arg(0));
                    let method = instruction.arg(1);
                    let start = instruction.arg(2);
                    let args = instruction.arg(3);

                    process::send_message(
                        &self.state,
                        generator,
                        client,
                        method,
                        start,
                        args,
                    );
                }
                Opcode::ProcessYield => {
                    process::yield_control(&self.state, process);

                    return Ok(());
                }
                Opcode::ProcessSuspend => {
                    let time =
                        generator.context.get_register(instruction.arg(0));

                    process::suspend(&self.state, process, time);

                    return Ok(());
                }
                Opcode::ObjectEquals => {
                    let reg = instruction.arg(0);
                    let a = generator.context.get_register(instruction.arg(1));
                    let b = generator.context.get_register(instruction.arg(2));
                    let res = object::equals(&self.state, a, b);

                    generator.context.set_register(reg, res);
                }
                Opcode::GetNil => {
                    let res = self.state.permanent_space.nil_singleton;

                    generator.context.set_register(instruction.arg(0), res);
                }
                Opcode::GetUndefined => {
                    let res = self.state.permanent_space.undefined_singleton;

                    generator.context.set_register(instruction.arg(0), res);
                }
                Opcode::SetGlobal => {
                    let idx = instruction.arg(0);
                    let val =
                        generator.context.get_register(instruction.arg(1));

                    general::set_global(generator, idx, val);
                }
                Opcode::GetGlobal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::get_global(generator, idx);

                    generator.context.set_register(reg, res);
                }
                Opcode::Throw => {
                    let value =
                        generator.context.get_register(instruction.arg(0));

                    throw_value!(self, generator, value);
                }
                Opcode::CopyRegister => {
                    let reg = instruction.arg(0);
                    let val =
                        generator.context.get_register(instruction.arg(1));

                    generator.context.set_register(reg, val);
                }
                Opcode::ProcessSetBlocking => {
                    let reg = instruction.arg(0);
                    let blocking =
                        generator.context.get_register(instruction.arg(1));
                    let res =
                        process::set_blocking(&self.state, process, blocking);

                    generator.context.set_register(reg, res);

                    if res == self.state.permanent_space.false_singleton {
                        continue;
                    }

                    // After this we can't perform any operations on the
                    // process any more as it might be concurrently modified by
                    // the pool we just moved it to.
                    self.state.scheduler.schedule(process);

                    return Ok(());
                }
                Opcode::Panic => {
                    let msg =
                        generator.context.get_register(instruction.arg(0));

                    vm_panic!(
                        unsafe { InkoString::read(&msg).to_string() },
                        generator
                    );
                }
                Opcode::Exit => {
                    let status =
                        generator.context.get_register(instruction.arg(0));

                    general::exit(&self.state, status)?;

                    return Ok(());
                }
                Opcode::StringConcat => {
                    let reg = instruction.arg(0);
                    let start = instruction.arg(1);
                    let len = instruction.arg(2);
                    let res = string::concat(
                        &self.state,
                        worker,
                        generator,
                        start,
                        len,
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ByteArrayAllocate => {
                    let reg = instruction.arg(0);
                    let start = instruction.arg(1);
                    let len = instruction.arg(2);
                    let res = byte_array::allocate(
                        &self.state,
                        worker,
                        generator,
                        start,
                        len,
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ByteArraySet => {
                    let ary =
                        generator.context.get_register(instruction.arg(0));
                    let idx =
                        generator.context.get_register(instruction.arg(1));
                    let val =
                        generator.context.get_register(instruction.arg(2));

                    try_error!(
                        byte_array::set(&self.state, ary, idx, val),
                        self,
                        worker,
                        generator
                    );
                }
                Opcode::ByteArrayGet => {
                    let reg = instruction.arg(0);
                    let ary =
                        generator.context.get_register(instruction.arg(1));
                    let idx =
                        generator.context.get_register(instruction.arg(2));
                    let res = try_error!(
                        byte_array::get(ary, idx),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ByteArrayRemove => {
                    let reg = instruction.arg(0);
                    let ary =
                        generator.context.get_register(instruction.arg(1));
                    let idx =
                        generator.context.get_register(instruction.arg(2));
                    let res = try_error!(
                        byte_array::remove(ary, idx),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ByteArrayLength => {
                    let reg = instruction.arg(0);
                    let ary =
                        generator.context.get_register(instruction.arg(1));
                    let res = byte_array::length(&self.state, worker, ary);

                    generator.context.set_register(reg, res);
                }
                Opcode::ByteArrayEquals => {
                    let reg = instruction.arg(0);
                    let cmp =
                        generator.context.get_register(instruction.arg(1));
                    let cmp_with =
                        generator.context.get_register(instruction.arg(2));
                    let res = byte_array::equals(&self.state, cmp, cmp_with);

                    generator.context.set_register(reg, res);
                }
                Opcode::ProcessSetPinned => {
                    let reg = instruction.arg(0);
                    let pin =
                        generator.context.get_register(instruction.arg(1));
                    let res =
                        process::set_pinned(&self.state, worker, process, pin);

                    generator.context.set_register(reg, res);
                }
                Opcode::StringByte => {
                    let reg = instruction.arg(0);
                    let val =
                        generator.context.get_register(instruction.arg(1));
                    let idx =
                        generator.context.get_register(instruction.arg(2));
                    let res = string::byte(val, idx);

                    generator.context.set_register(reg, res);
                }
                Opcode::MoveResult => {
                    let reg = instruction.arg(0);
                    let res = general::move_result(&mut generator)?;

                    generator.context.set_register(reg, res);
                }
                Opcode::GeneratorAllocate => {
                    let reg = instruction.arg(0);
                    let rec =
                        generator.context.get_register(instruction.arg(1));
                    let idx = instruction.arg(2);
                    let start = instruction.arg(3);
                    let args = instruction.arg(4);
                    let res = generator::allocate(
                        &self.state,
                        worker,
                        generator,
                        rec,
                        idx,
                        start,
                        args,
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::GeneratorResume => {
                    let gen =
                        generator.context.get_register(instruction.arg(0));

                    generator::resume(process, gen)?;
                }
                Opcode::GeneratorYield => {
                    let val =
                        generator.context.get_register(instruction.arg(0));

                    generator.set_yield_value(val);

                    if process.pop_generator() {
                        vm_panic!(
                            "Can't yield from the root generator".to_string(),
                            generator
                        );
                    }

                    reduce!(self, process, reductions);
                }
                Opcode::GeneratorValue => {
                    let reg = instruction.arg(0);
                    let gen =
                        generator.context.get_register(instruction.arg(1));
                    let res = try_error!(
                        generator::value(&self.state, gen),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ExternalFunctionCall => {
                    let reg = instruction.arg(0);
                    let func =
                        generator.context.get_register(instruction.arg(1));
                    let start = instruction.arg(2);
                    let len = instruction.arg(3);
                    let res = try_io_error!(
                        external_functions::call(
                            &self.state,
                            worker,
                            process,
                            generator,
                            func,
                            start,
                            len
                        ),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::ExternalFunctionLoad => {
                    let reg = instruction.arg(0);
                    let name =
                        generator.context.get_register(instruction.arg(1));
                    let res = external_functions::load(&self.state, name)?;

                    generator.context.set_register(reg, res);
                }
                Opcode::FutureAllocate => {
                    let read_reg = instruction.arg(0);
                    let write_reg = instruction.arg(1);
                    let (read, write) = future::allocate(&self.state, worker);

                    generator.context.set_register(read_reg, read);
                    generator.context.set_register(write_reg, write);
                }
                Opcode::FutureGet => {
                    let reg = instruction.arg(0);
                    let fut =
                        generator.context.get_register(instruction.arg(1));

                    // Save the instruction offset first, so rescheduled copies
                    // of our process start off at the right instruction.
                    generator.context.index -= 1;

                    let result = try_error!(
                        future::get(&self.state, process, fut),
                        self,
                        worker,
                        generator
                    );

                    if let Some(message) = result {
                        generator.context.index += 1;

                        generator.context.set_register(reg, message);
                    } else {
                        return Ok(());
                    }
                }
                Opcode::FutureGetWithTimeout => {
                    let reg = instruction.arg(0);
                    let fut =
                        generator.context.get_register(instruction.arg(1));
                    let time =
                        generator.context.get_register(instruction.arg(2));

                    // Save the instruction offset first, so rescheduled copies
                    // of our process start off at the right instruction.
                    generator.context.index -= 1;

                    let res = try_error!(
                        future::get_with_timeout(
                            &self.state,
                            process,
                            fut,
                            time
                        ),
                        self,
                        worker,
                        generator
                    );

                    if let Some(message) = res {
                        generator.context.index += 1;

                        generator.context.set_register(reg, message);
                    } else {
                        return Ok(());
                    }
                }
                Opcode::FutureWrite => {
                    let reg = instruction.arg(0);
                    let fut =
                        generator.context.get_register(instruction.arg(1));
                    let val =
                        generator.context.get_register(instruction.arg(2));
                    let throw = instruction.arg(3) == 1;
                    let res = try_error!(
                        future::write(&self.state, process, fut, val, throw),
                        self,
                        worker,
                        generator
                    );

                    generator.context.set_register(reg, res);
                }
                Opcode::StaticCall => {
                    let rec =
                        generator.context.get_register(instruction.arg(1));
                    let idx = instruction.arg(2);
                    let start = instruction.arg(3);
                    let args = instruction.arg(4);

                    object::static_call(
                        &self.state,
                        &mut generator,
                        rec,
                        idx,
                        start,
                        args,
                    );
                }
                Opcode::DynamicCall => {
                    let rec =
                        generator.context.get_register(instruction.arg(1));
                    let hash1 = instruction.arg(2);
                    let hash2 = instruction.arg(3);
                    let start = instruction.arg(4);
                    let args = instruction.arg(5);

                    object::dynamic_call(
                        &self.state,
                        &mut generator,
                        rec,
                        hash1,
                        hash2,
                        start,
                        args,
                    );
                }
                Opcode::MethodGet => {
                    let reg = instruction.arg(0);
                    let rec =
                        generator.context.get_register(instruction.arg(1));
                    let idx = instruction.arg(2);
                    let res = object::get_method(&self.state, rec, idx);

                    generator.context.set_register(reg, res);
                }
                Opcode::IncrementRef => {
                    let obj =
                        generator.context.get_register(instruction.arg(0));

                    general::increment_ref(obj);
                }
                Opcode::DecrementRef => {
                    let obj =
                        generator.context.get_register(instruction.arg(0));

                    general::decrement_ref(obj);
                }
                Opcode::Drop => {
                    let obj =
                        generator.context.get_register(instruction.arg(0));

                    try_error!(general::drop(obj), self, worker, generator);
                }
            };
        }

        process::finish_generator(&self.state, worker, process);
        Ok(())
    }

    fn panic(
        &self,
        process: ServerPointer,
        generator: GeneratorPointer,
        message: &str,
    ) {
        let mut frames = Vec::new();
        let mut buffer = String::new();

        for context in generator.contexts() {
            frames.push(format!(
                "\"{}\" line {}, in \"{}\"",
                context.method.file,
                context.line().to_string(),
                context.method.name
            ));
        }

        frames.reverse();

        buffer.push_str("Stack trace (the most recent call comes last):");

        for (index, line) in frames.iter().enumerate() {
            buffer.push_str(&format!("\n  {}: {}", index, line));
        }

        buffer.push_str(&format!(
            "\nProcess {:#x} panicked: {}",
            process.identifier(),
            message
        ));

        eprintln!("{}", buffer);
        self.state.terminate(1);
    }
}
