//! Virtual Machine States
//!
//! Each virtual machine has its own state. This state includes any scheduled
//! garbage collections, the configuration, the files that have been parsed,
//! etc.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::config::Config;
use crate::external_functions::ExternalFunctions;
use crate::mem::allocator::Pointer;
use crate::mem::permanent_space::PermanentSpace;
use crate::network_poller::NetworkPoller;
use crate::scheduler::process_scheduler::ProcessScheduler;
use crate::scheduler::timeout_worker::TimeoutWorker;
use parking_lot::Mutex;
use std::panic::RefUnwindSafe;
use std::time;

/// A reference counted State.
pub type RcState = ArcWithoutWeak<State>;

/// The state of a virtual machine.
pub struct State {
    /// The virtual machine's configuration.
    pub config: Config,

    /// The start time of the VM (more or less).
    pub start_time: time::Instant,

    /// The commandline arguments passed to an Inko program.
    pub arguments: Vec<Pointer>,

    /// The exit status to use when the VM terminates.
    pub exit_status: Mutex<i32>,

    /// The scheduler to use for executing Inko processes.
    pub scheduler: ProcessScheduler,

    /// A task used for handling timeouts, such as message and IO timeouts.
    pub timeout_worker: TimeoutWorker,

    /// The system polling mechanism to use for polling non-blocking sockets.
    pub network_poller: NetworkPoller,

    /// All external functions that a compiler can use.
    pub external_functions: ExternalFunctions,

    /// A type for allocating and storing blocks and permanent objects.
    pub permanent_space: PermanentSpace,
}

impl RefUnwindSafe for State {}

impl State {
    pub fn new(
        config: Config,
        permanent_space: PermanentSpace,
        args: &[String],
    ) -> RcState {
        let external_functions = ExternalFunctions::setup()
            .expect("Failed to set up the default external functions");

        let arguments = args
            .iter()
            .map(|arg| permanent_space.allocate_string(arg.clone()))
            .collect();

        let state = State {
            scheduler: ProcessScheduler::new(
                config.primary_threads,
                config.blocking_threads,
            ),
            config,
            start_time: time::Instant::now(),
            exit_status: Mutex::new(0),
            timeout_worker: TimeoutWorker::new(),
            arguments,
            network_poller: NetworkPoller::new(),
            external_functions,
            permanent_space,
        };

        ArcWithoutWeak::new(state)
    }

    pub fn terminate(&self, status: i32) {
        self.set_exit_status(status);
        self.scheduler.terminate();
        self.timeout_worker.terminate();
        self.network_poller.terminate();
    }

    pub fn set_exit_status(&self, new_status: i32) {
        *self.exit_status.lock() = new_status;
    }

    pub fn current_exit_status(&self) -> i32 {
        *self.exit_status.lock()
    }
}
