//! Virtual Machine for running instructions
use crate::gc::collection::collect as collect_garbage;
use crate::integer_operations;
use crate::network_poller::Worker as NetworkPollerWorker;
use crate::numeric::division::{FlooredDiv, OverflowingFlooredDiv};
use crate::numeric::modulo::{Modulo, OverflowingModulo};
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::scheduler::join_list::JoinList;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::instruction::Opcode;
use crate::vm::instructions::array;
use crate::vm::instructions::block;
use crate::vm::instructions::byte_array;
use crate::vm::instructions::external_functions;
use crate::vm::instructions::float;
use crate::vm::instructions::general;
use crate::vm::instructions::generator;
use crate::vm::instructions::module;
use crate::vm::instructions::object;
use crate::vm::instructions::process;
use crate::vm::instructions::string;
use crate::vm::state::RcState;
use num_bigint::BigInt;
use std::i32;
use std::ops::{Add, Mul, Sub};
use std::thread;

/// The number of reductions to apply for a method call.
const METHOD_REDUCTION_COST: usize = 1;

/// The base number of reductions to apply for a garbage collection cycle.
///
/// This value is chosen because GC cycles are more expensive than method calls.
/// Other than that, it's entirely arbitrary.
const GC_REDUCTION_COST: usize = 100;

macro_rules! reset_context {
    ($process:expr, $context:ident, $index:ident) => {{
        $context = $process.context_mut();
        $index = $context.instruction_index;
    }};
}

macro_rules! remember_and_reset {
    ($process: expr, $context: ident, $index: ident) => {
        $context.instruction_index = $index - 1;

        reset_context!($process, $context, $index);
        continue;
    };
}

macro_rules! throw_value {
    (
        $machine:expr,
        $process:expr,
        $value:expr,
        $context:ident,
        $index:ident
    ) => {{
        $context.instruction_index = $index;

        $machine.throw($process, $value)?;

        reset_context!($process, $context, $index);
        continue;
    }};
}

macro_rules! throw_error_message {
    (
        $machine:expr,
        $process:expr,
        $message:expr,
        $context:ident,
        $index:ident
    ) => {{
        let value = $process.allocate(
            object_value::string($message),
            $machine.state.string_prototype,
        );

        throw_value!($machine, $process, value, $context, $index);
    }};
}

macro_rules! enter_context {
    ($process:expr, $context:ident, $index:ident) => {{
        $context.instruction_index = $index;

        reset_context!($process, $context, $index);
    }};
}

macro_rules! safepoint_and_reduce {
    ($vm:expr, $worker:expr, $process:expr, $reductions:expr) => {{
        let reduce_by = $vm.gc_safepoint(&$process, $worker);

        if $reductions >= reduce_by {
            $reductions -= reduce_by;
        } else {
            $vm.state.scheduler.schedule($process.clone());
            return Ok(());
        }
    }};
}

/// Handles an operation that may produce IO errors.
macro_rules! try_io_error {
    ($expr:expr, $vm:expr, $proc:expr, $context:ident, $index:ident) => {{
        // When an operation would block, the socket is already registered, and
        // the process may already be running again in another thread. This
        // means that when a WouldBlock is produced it is not safe to access any
        // process data.
        //
        // To ensure blocking operations are retried properly, we _first_ set
        // the instruction index, then advance it again if it is safe to do so.
        $context.instruction_index = $index - 1;

        match $expr {
            Ok(thing) => {
                $context.instruction_index = $index;

                thing
            }
            Err(RuntimeError::Panic(msg)) => {
                vm_panic!(msg, $context, $index);
            }
            Err(RuntimeError::ErrorMessage(msg)) => {
                throw_error_message!($vm, $proc, msg, $context, $index);
            }
            Err(RuntimeError::Error(err)) => {
                throw_value!($vm, $proc, err, $context, $index);
            }
            Err(RuntimeError::WouldBlock) => {
                // *DO NOT* use "$context" at this point, as it may have been
                // invalidated if the process is already running again in
                // another thread.
                return Ok(());
            }
        }
    }};
}

/// Handles a regular runtime error.
macro_rules! try_error {
    ($expr:expr, $vm:expr, $proc:expr, $context:ident, $index:ident) => {{
        match $expr {
            Ok(thing) => thing,
            Err(RuntimeError::Panic(msg)) => {
                vm_panic!(msg, $context, $index);
            }
            Err(RuntimeError::ErrorMessage(msg)) => {
                throw_error_message!($vm, $proc, msg, $context, $index);
            }
            Err(RuntimeError::Error(err)) => {
                throw_value!($vm, $proc, err, $context, $index);
            }
            _ => unreachable!(),
        }
    }};
}

macro_rules! vm_panic {
    ($message:expr, $context:expr, $index:expr) => {{
        // We subtract one so the instruction pointer points to the current
        // instruction (= the panic), not the one we'd run after that.
        $context.instruction_index = $index - 1;

        return Err($message);
    }};
}

#[derive(Clone)]
pub struct Machine {
    /// The shared virtual machine state, such as the process pools and built-in
    /// types.
    pub state: RcState,
}

impl Machine {
    pub fn new(state: RcState) -> Self {
        Machine { state }
    }

    /// Starts the VM
    ///
    /// This method will block the calling thread until the program finishes.
    pub fn start(&self, path: &str) {
        self.parse_image(path);
        self.schedule_main_process();

        let secondary_guard = self.start_blocking_threads();
        let timeout_guard = self.start_timeout_worker_thread();

        // The network poller doesn't produce a guard, because there's no
        // cross-platform way of waking up the system poller, so we just don't
        // wait for it to finish when terminating.
        let poller_guard = self.start_network_poller_thread();

        // Starting the primary threads will block this thread, as the main
        // worker will run directly onto the current thread. As such, we must
        // start these threads last.
        let primary_guard = self.start_primary_threads();

        // Joining the pools only fails in case of a panic. In this case we
        // don't want to re-panic as this clutters the error output.
        if primary_guard.join().is_err()
            || secondary_guard.join().is_err()
            || timeout_guard.join().is_err()
            || poller_guard.join().is_err()
        {
            self.state.set_exit_status(1);
        }
    }

    fn start_primary_threads(&self) -> JoinList<()> {
        self.state.scheduler.primary_pool.start_main(self.clone())
    }

    fn start_blocking_threads(&self) -> JoinList<()> {
        self.state.scheduler.blocking_pool.start(self.clone())
    }

    fn start_timeout_worker_thread(&self) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::Builder::new()
            .name("timeout worker".to_string())
            .spawn(move || {
                state.timeout_worker.run(&state.scheduler);
            })
            .unwrap()
    }

    fn start_network_poller_thread(&self) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::Builder::new()
            .name("network poller".to_string())
            .spawn(move || {
                NetworkPollerWorker::new(state).run();
            })
            .unwrap()
    }

    fn parse_image(&self, path: &str) {
        self.state.parse_image(path).unwrap();
    }

    fn schedule_main_process(&self) {
        let entry = self
            .state
            .modules
            .lock()
            .entry_point()
            .expect("The module entry point is undefined")
            .clone();

        let process = {
            let (_, block, _) =
                module::module_load_string(&self.state, &entry).unwrap();

            process::process_allocate(&self.state, &block)
        };

        process.set_main();
        self.state.scheduler.schedule_on_main_thread(process);
    }

    pub fn run(&mut self, worker: &mut ProcessWorker, process: &RcProcess) {
        if let Err(message) = self.run_loop(worker, process) {
            self.panic(process, &message);
        }
    }

    #[cfg_attr(
        feature = "cargo-clippy",
        allow(cyclomatic_complexity, cognitive_complexity)
    )]
    fn run_loop(
        &mut self,
        worker: &mut ProcessWorker,
        process: &RcProcess,
    ) -> Result<(), String> {
        let mut reductions = self.state.config.reductions;
        let mut context;
        let mut index;
        let mut instruction;

        reset_context!(process, context, index);

        'exec_loop: loop {
            instruction = unsafe { context.code.instruction(index) };
            index += 1;

            match instruction.opcode {
                Opcode::SetLiteral => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::set_literal(context, idx);

                    context.set_register(reg, res);
                }
                Opcode::SetLiteralWide => {
                    let reg = instruction.arg(0);
                    let arg1 = instruction.arg(1);
                    let arg2 = instruction.arg(2);
                    let res = general::set_literal_wide(context, arg1, arg2);

                    context.set_register(reg, res);
                }
                Opcode::Allocate => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let res = object::allocate(process, proto);

                    context.set_register(reg, res);
                }
                Opcode::AllocatePermanent => {
                    let reg = instruction.arg(0);
                    let proto = context.get_register(instruction.arg(1));
                    let res = try_error!(
                        object::allocate_permanent(&self.state, proto),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ArrayAllocate => {
                    let reg = instruction.arg(0);
                    let start = instruction.arg(1);
                    let len = instruction.arg(2);
                    let res = array::array_allocate(
                        &self.state,
                        process,
                        context,
                        start,
                        len,
                    );

                    context.set_register(reg, res);
                }
                Opcode::GetBuiltinPrototype => {
                    let reg = instruction.arg(0);
                    let id = context.get_register(instruction.arg(1));
                    let proto = object::get_builtin_prototype(&self.state, id)?;

                    context.set_register(reg, proto);
                }
                Opcode::GetTrue => {
                    let res = self.state.true_object;

                    context.set_register(instruction.arg(0), res);
                }
                Opcode::GetFalse => {
                    let res = self.state.false_object;

                    context.set_register(instruction.arg(0), res);
                }
                Opcode::SetLocal => {
                    let idx = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));

                    general::set_local(context, idx, val);
                }
                Opcode::GetLocal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::get_local(context, idx);

                    context.set_register(reg, res);
                }
                Opcode::SetBlock => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let rec = context.get_register(instruction.arg(2));
                    let res = block::set_block(
                        &self.state,
                        process,
                        context,
                        idx,
                        rec,
                    );

                    context.set_register(reg, res);
                }
                Opcode::Return => {
                    // If there are any pending deferred blocks, execute these
                    // first, then retry this instruction.
                    if context.schedule_deferred_blocks(process)? {
                        remember_and_reset!(process, context, index);
                    }

                    let method_return = instruction.arg(0) == 1;
                    let res = context.get_register(instruction.arg(1));

                    process.set_result(res);

                    if method_return {
                        process::process_unwind_until_defining_scope(process);
                    }

                    // Once we're at the top-level _and_ we have no more
                    // instructions to process, we'll write the result to a
                    // future and bail out the execution loop.
                    if process.pop_context() {
                        process::future_set_result(
                            &self.state,
                            process,
                            res,
                            false,
                        )?;

                        break 'exec_loop;
                    }

                    reset_context!(process, context, index);
                    safepoint_and_reduce!(self, worker, process, reductions);
                }
                Opcode::GotoIfFalse => {
                    let val = context.get_register(instruction.arg(1));

                    if is_false!(self.state, val) {
                        index = instruction.arg(0) as usize;
                    }
                }
                Opcode::GotoIfTrue => {
                    let val = context.get_register(instruction.arg(1));

                    if !is_false!(self.state, val) {
                        index = instruction.arg(0) as usize;
                    }
                }
                Opcode::Goto => {
                    index = instruction.arg(0) as usize;
                }
                Opcode::IntegerAdd => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        add,
                        overflowing_add
                    );
                }
                Opcode::IntegerDiv => {
                    if context
                        .get_register(instruction.arg(2))
                        .is_zero_integer()
                    {
                        vm_panic!(
                            "Can not divide an Integer by 0".to_string(),
                            context,
                            index
                        );
                    }

                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        floored_division,
                        overflowing_floored_division
                    );
                }
                Opcode::IntegerMul => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        mul,
                        overflowing_mul
                    );
                }
                Opcode::IntegerSub => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        sub,
                        overflowing_sub
                    );
                }
                Opcode::IntegerMod => {
                    integer_overflow_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        modulo,
                        overflowing_modulo
                    );
                }
                Opcode::IntegerBitwiseAnd => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        &
                    );
                }
                Opcode::IntegerBitwiseOr => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        |
                    );
                }
                Opcode::IntegerBitwiseXor => {
                    integer_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        ^
                    );
                }
                Opcode::IntegerShiftLeft => {
                    integer_shift_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        integer_shift_left,
                        bigint_shift_left
                    );
                }
                Opcode::IntegerShiftRight => {
                    integer_shift_op!(
                        process,
                        context,
                        self.state.integer_prototype,
                        instruction,
                        integer_shift_right,
                        bigint_shift_right
                    );
                }
                Opcode::IntegerSmaller => {
                    integer_bool_op!(self.state, context, instruction, <);
                }
                Opcode::IntegerGreater => {
                    integer_bool_op!(self.state, context, instruction, >);
                }
                Opcode::IntegerEquals => {
                    integer_bool_op!(self.state, context, instruction, ==);
                }
                Opcode::IntegerGreaterOrEqual => {
                    integer_bool_op!(self.state, context, instruction, >=);
                }
                Opcode::IntegerSmallerOrEqual => {
                    integer_bool_op!(self.state, context, instruction, <=);
                }
                Opcode::FloatAdd => {
                    float_op!(self.state, process, context, instruction, +);
                }
                Opcode::FloatMul => {
                    float_op!(self.state, process, context, instruction, *);
                }
                Opcode::FloatDiv => {
                    float_op!(self.state, process, context, instruction, /);
                }
                Opcode::FloatSub => {
                    float_op!(self.state, process, context, instruction, -);
                }
                Opcode::FloatMod => {
                    float_op!(self.state, process, context, instruction, %);
                }
                Opcode::FloatSmaller => {
                    float_bool_op!(self.state, context, instruction, <);
                }
                Opcode::FloatGreater => {
                    float_bool_op!(self.state, context, instruction, >);
                }
                Opcode::FloatEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res = float::float_equals(&self.state, cmp, cmp_with)?;

                    context.set_register(reg, res);
                }
                Opcode::FloatGreaterOrEqual => {
                    float_bool_op!(self.state, context, instruction, >=);
                }
                Opcode::FloatSmallerOrEqual => {
                    float_bool_op!(self.state, context, instruction, <=);
                }
                Opcode::ArraySet => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = try_error!(
                        array::array_set(&self.state, process, ary, idx, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ArrayAt => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = try_error!(
                        array::array_get(ary, idx),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ArrayRemove => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = try_error!(
                        array::array_remove(ary, idx),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ArrayLength => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let res = array::array_length(&self.state, process, ary)?;

                    context.set_register(reg, res);
                }
                Opcode::StringEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res =
                        string::string_equals(&self.state, cmp, cmp_with)?;

                    context.set_register(reg, res);
                }
                Opcode::StringLength => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = string::string_length(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::StringSize => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let res = string::string_size(&self.state, process, val)?;

                    context.set_register(reg, res);
                }
                Opcode::ModuleLoad => {
                    let reg = instruction.arg(0);
                    let name = context.get_register(instruction.arg(1));
                    let res = module::module_load(&self.state, process, name)?;

                    context.set_register(reg, res);
                    enter_context!(process, context, index);
                }
                Opcode::ModuleGet => {
                    let reg = instruction.arg(0);
                    let name = context.get_register(instruction.arg(1));
                    let res = module::module_get(&self.state, name)?;

                    context.set_register(reg, res);
                }
                Opcode::SetAttribute => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = try_error!(
                        object::set_attribute(
                            &self.state,
                            process,
                            rec,
                            name,
                            val,
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::GetAttribute => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res = object::get_attribute(&self.state, rec, name);

                    context.set_register(reg, res);
                }
                Opcode::GetAttributeInSelf => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res =
                        object::get_attribute_in_self(&self.state, rec, name);

                    context.set_register(reg, res);
                }
                Opcode::GetPrototype => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let res = object::get_prototype(&self.state, src);

                    context.set_register(reg, res);
                }
                Opcode::LocalExists => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::local_exists(&self.state, context, idx);

                    context.set_register(reg, res);
                }
                Opcode::ProcessSpawn => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let res =
                        process::process_spawn(&self.state, process, rec)?;

                    context.set_register(reg, res);
                }
                Opcode::ProcessSendMessage => {
                    let reg = instruction.arg(0);
                    let rec = context.get_register(instruction.arg(1));
                    let msg = context.get_register(instruction.arg(2));
                    let start = instruction.arg(3);
                    let args = instruction.arg(4);
                    let res = try_error!(
                        process::process_send_message(
                            &self.state,
                            context,
                            process,
                            rec,
                            msg,
                            start,
                            args
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ProcessSuspendCurrent => {
                    let time = context.get_register(instruction.arg(0));

                    context.instruction_index = index;

                    process::process_suspend_current(
                        &self.state,
                        process,
                        time,
                    )?;

                    return Ok(());
                }
                Opcode::SetParentLocal => {
                    let idx = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let val = context.get_register(instruction.arg(2));

                    general::set_parent_local(context, idx, depth, val)?;
                }
                Opcode::GetParentLocal => {
                    let reg = instruction.arg(0);
                    let depth = instruction.arg(1);
                    let idx = instruction.arg(2);
                    let res = general::get_parent_local(context, idx, depth)?;

                    context.set_register(reg, res)
                }
                Opcode::ObjectEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res = object::object_equals(&self.state, cmp, cmp_with);

                    context.set_register(reg, res);
                }
                Opcode::GetNil => {
                    context.set_register(
                        instruction.arg(0),
                        self.state.nil_object,
                    );
                }
                Opcode::AttributeExists => {
                    let reg = instruction.arg(0);
                    let src = context.get_register(instruction.arg(1));
                    let name = context.get_register(instruction.arg(2));
                    let res = object::attribute_exists(&self.state, src, name);

                    context.set_register(reg, res);
                }
                Opcode::RunBlock => {
                    let block = context.get_register(instruction.arg(0));
                    let start = instruction.arg(1);
                    let args = instruction.arg(2);

                    block::run_block(process, context, block, start, args)?;
                    enter_context!(process, context, index);
                }
                Opcode::SetGlobal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let val = context.get_register(instruction.arg(2));
                    let res = try_error!(
                        general::set_global(&self.state, context, idx, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::GetGlobal => {
                    let reg = instruction.arg(0);
                    let idx = instruction.arg(1);
                    let res = general::get_global(context, idx);

                    context.set_register(reg, res);
                }
                Opcode::Throw => {
                    let method_throw = instruction.arg(0) == 1;
                    let value = context.get_register(instruction.arg(1));

                    if method_throw {
                        process::process_unwind_until_defining_scope(process);
                    }

                    throw_value!(self, process, value, context, index);
                }
                Opcode::CopyRegister => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));

                    context.set_register(reg, val);
                }
                Opcode::TailCall => {
                    let start = instruction.arg(0);
                    let args = instruction.arg(1);

                    block::tail_call(context, start, args);
                    reset_context!(process, context, index);
                    safepoint_and_reduce!(self, worker, process, reductions);
                }
                Opcode::CopyBlocks => {
                    let to = context.get_register(instruction.arg(0));
                    let from = context.get_register(instruction.arg(1));

                    try_error!(
                        object::copy_blocks(&self.state, to, from),
                        self,
                        process,
                        context,
                        index
                    );
                }
                Opcode::Close => {
                    let ptr = context.get_register(instruction.arg(0));

                    object::close(ptr);
                }
                Opcode::ProcessSetBlocking => {
                    let reg = instruction.arg(0);
                    let blocking = context.get_register(instruction.arg(1));
                    let res = process::process_set_blocking(
                        &self.state,
                        process,
                        blocking,
                    );

                    context.set_register(reg, res);

                    if res == self.state.false_object {
                        continue;
                    }

                    context.instruction_index = index;

                    // After this we can _not_ perform any operations on the
                    // process any more as it might be concurrently modified by
                    // the pool we just moved it to.
                    self.state.scheduler.schedule(process.clone());

                    return Ok(());
                }
                Opcode::Panic => {
                    let msg = context.get_register(instruction.arg(0));

                    vm_panic!(
                        msg.string_value()?.to_owned_string(),
                        context,
                        index
                    );
                }
                Opcode::Exit => {
                    // Any pending deferred blocks should be executed first.
                    if context
                        .schedule_deferred_blocks_of_all_parents(process)?
                    {
                        remember_and_reset!(process, context, index);
                    }

                    let status = context.get_register(instruction.arg(0));

                    general::exit(&self.state, status)?;

                    return Ok(());
                }
                Opcode::StringConcat => {
                    let reg = instruction.arg(0);
                    let start = instruction.arg(1);
                    let len = instruction.arg(2);
                    let res = string::string_concat(
                        &self.state,
                        process,
                        context,
                        start,
                        len,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ProcessTerminateCurrent => {
                    break 'exec_loop;
                }
                Opcode::ByteArrayFromArray => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let res = byte_array::byte_array_from_array(
                        &self.state,
                        process,
                        ary,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArraySet => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let val = context.get_register(instruction.arg(3));
                    let res = try_error!(
                        byte_array::byte_array_set(ary, idx, val),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayAt => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = try_error!(
                        byte_array::byte_array_get(ary, idx),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayRemove => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = try_error!(
                        byte_array::byte_array_remove(ary, idx),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayLength => {
                    let reg = instruction.arg(0);
                    let ary = context.get_register(instruction.arg(1));
                    let res = byte_array::byte_array_length(
                        &self.state,
                        process,
                        ary,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::ByteArrayEquals => {
                    let reg = instruction.arg(0);
                    let cmp = context.get_register(instruction.arg(1));
                    let cmp_with = context.get_register(instruction.arg(2));
                    let res = byte_array::byte_array_equals(
                        &self.state,
                        cmp,
                        cmp_with,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::BlockGetReceiver => {
                    let reg = instruction.arg(0);
                    let res = block::block_get_receiver(context);

                    context.set_register(reg, res);
                }
                Opcode::RunBlockWithReceiver => {
                    let block = context.get_register(instruction.arg(0));
                    let rec = context.get_register(instruction.arg(1));
                    let start = instruction.arg(2);
                    let args = instruction.arg(3);

                    block::run_block_with_receiver(
                        process, context, block, rec, start, args,
                    )?;

                    enter_context!(process, context, index);
                }
                Opcode::ProcessAddDeferToCaller => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let res =
                        process::process_add_defer_to_caller(process, block)?;

                    context.set_register(reg, res);
                }
                Opcode::ProcessSetPinned => {
                    let reg = instruction.arg(0);
                    let pin = context.get_register(instruction.arg(1));
                    let res = process::process_set_pinned(
                        &self.state,
                        process,
                        worker,
                        pin,
                    );

                    context.set_register(reg, res);
                }
                Opcode::ProcessIdentifier => {
                    let reg = instruction.arg(0);
                    let proc = context.get_register(instruction.arg(1));
                    let res = process::process_identifier(
                        &self.state,
                        process,
                        proc,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::StringByte => {
                    let reg = instruction.arg(0);
                    let val = context.get_register(instruction.arg(1));
                    let idx = context.get_register(instruction.arg(2));
                    let res = string::string_byte(val, idx)?;

                    context.set_register(reg, res);
                }
                Opcode::MoveResult => {
                    let reg = instruction.arg(0);
                    let res = general::move_result(process)?;

                    context.set_register(reg, res);
                }
                Opcode::GeneratorAllocate => {
                    let reg = instruction.arg(0);
                    let block = context.get_register(instruction.arg(1));
                    let rec = context.get_register(instruction.arg(2));
                    let start = instruction.arg(3);
                    let args = instruction.arg(4);
                    let res = generator::allocate(
                        &self.state,
                        process,
                        context,
                        block,
                        rec,
                        start,
                        args,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::GeneratorResume => {
                    let gen = context.get_register(instruction.arg(0));

                    generator::resume(process, gen)?;
                    enter_context!(process, context, index);
                }
                Opcode::GeneratorYield => {
                    let val = context.get_register(instruction.arg(0));

                    if !process.yield_value(val) {
                        vm_panic!(
                            "Can't yield from the top-level generator"
                                .to_string(),
                            context,
                            index
                        );
                    }

                    enter_context!(process, context, index);
                    safepoint_and_reduce!(self, worker, process, reductions);
                }
                Opcode::GeneratorValue => {
                    let reg = instruction.arg(0);
                    let gen = context.get_register(instruction.arg(1));
                    let res = try_error!(
                        generator::value(gen),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ExternalFunctionCall => {
                    let reg = instruction.arg(0);
                    let func = context.get_register(instruction.arg(1));
                    let start = instruction.arg(2);
                    let len = instruction.arg(3);
                    let res = try_io_error!(
                        external_functions::external_function_call(
                            &self.state,
                            process,
                            context,
                            func,
                            start,
                            len
                        ),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::ExternalFunctionLoad => {
                    let reg = instruction.arg(0);
                    let name = context.get_register(instruction.arg(1));
                    let res = external_functions::external_function_load(
                        &self.state,
                        name,
                    )?;

                    context.set_register(reg, res);
                }
                Opcode::FutureGet => {
                    let reg = instruction.arg(0);
                    let fut = context.get_register(instruction.arg(1));
                    let time = context.get_register(instruction.arg(2));

                    // Save the instruction offset first, so rescheduled copies
                    // of our process start off at the right instruction.
                    context.instruction_index = index - 1;

                    let res = try_error!(
                        process::future_get(&self.state, process, fut, time),
                        self,
                        process,
                        context,
                        index
                    );

                    if let Some(message) = res {
                        context.instruction_index = index;

                        context.set_register(reg, message);
                    } else {
                        return Ok(());
                    }
                }
                Opcode::FutureReady => {
                    let reg = instruction.arg(0);
                    let fut = context.get_register(instruction.arg(1));
                    let res = try_error!(
                        process::future_ready(&self.state, fut),
                        self,
                        process,
                        context,
                        index
                    );

                    context.set_register(reg, res);
                }
                Opcode::FutureWait => {
                    let reg = instruction.arg(0);
                    let poll = context.get_register(instruction.arg(1));
                    let time = context.get_register(instruction.arg(2));

                    // Save the instruction offset first, so rescheduled copies
                    // of our process start off at the right instruction.
                    context.instruction_index = index - 1;

                    let res = try_error!(
                        process::future_wait(&self.state, process, poll, time),
                        self,
                        process,
                        context,
                        index
                    );

                    if let Some(amount) = res {
                        context.instruction_index = index;

                        context.set_register(reg, amount);
                    } else {
                        return Ok(());
                    }
                }
                Opcode::IsNull => {
                    let reg = instruction.arg(0);
                    let ptr = context.get_register(1);
                    let res = general::is_null(&self.state, ptr);

                    context.set_register(reg, res);
                }
            };
        }

        process::process_finish_message(&self.state, worker, process)
    }

    /// Checks if a garbage collection run should be performed for the given
    /// process.
    ///
    /// This method returns the number of reductions to apply.
    fn gc_safepoint(
        &self,
        process: &RcProcess,
        worker: &ProcessWorker,
    ) -> usize {
        if process.should_collect_young_generation() {
            collect_garbage(&self.state, &process, &worker.tracers);
            GC_REDUCTION_COST
        } else {
            METHOD_REDUCTION_COST
        }
    }

    fn throw(
        &self,
        process: &RcProcess,
        value: ObjectPointer,
    ) -> Result<(), String> {
        let mut deferred = Vec::new();

        loop {
            let context = process.context_mut();
            let code = context.code;
            let index = context.instruction_index;

            for entry in &code.catch_table.entries {
                if entry.start < index && entry.end >= index {
                    context.instruction_index = entry.jump_to;

                    // When unwinding, move all deferred blocks to the context
                    // that handles the error. This makes unwinding easier, at
                    // the cost of making a return from this context slightly
                    // more expensive.
                    context.append_deferred_blocks(&mut deferred);
                    process.set_result(value);

                    return Ok(());
                }
            }

            if context.parent().is_some() {
                context.move_deferred_blocks_to(&mut deferred);
            }

            if process.pop_context() {
                // Move all the pending deferred blocks from previous frames
                // into the top-level frame. These will be scheduled once we
                // return from the panic handler.
                process.context_mut().append_deferred_blocks(&mut deferred);

                // TODO: this will discard the pending deferred blocks.
                // TODO: rip out unwinding. Or maybe turn throws into a return
                // with a flag set, then add an instruction that checks and
                // consumes that flag, then combine that with a jump in the
                // compiler.
                if !process::future_set_result(
                    &self.state,
                    process,
                    value,
                    true,
                )? {
                    return Err(format!(
                        "A thrown value reached the top-level in process {:#x}",
                        process.identifier()
                    ));
                }
            }
        }
    }

    fn panic(&mut self, process: &RcProcess, message: &str) {
        let mut frames = Vec::new();
        let mut buffer = String::new();

        for context in process.contexts() {
            frames.push(format!(
                "\"{}\" line {}, in \"{}\"",
                context.code.file.string_value().unwrap(),
                context.line().to_string(),
                context.code.name.string_value().unwrap()
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
