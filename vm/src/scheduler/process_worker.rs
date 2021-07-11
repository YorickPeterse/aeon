//! Executing of lightweight Inko processes in a single thread.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::mem::allocator::BumpAllocator;
use crate::mem::objects::MethodPointer;
use crate::mem::process::{Server, ServerPointer};
use crate::scheduler::pool_state::PoolState;
use crate::scheduler::queue::RcQueue;
use crate::vm::machine::Machine;
use crate::vm::state::State as VmState;
use std::ops::Drop;

/// The state that a worker is in.
#[derive(Eq, PartialEq, Debug)]
pub enum Mode {
    /// The worker should process its own queue or other queues in a normal
    /// fashion.
    Normal,

    /// The worker should only process a particular job, and not steal any other
    /// jobs.
    Exclusive,
}

/// A worker owned by a thread, used for executing jobs from a scheduler queue.
pub struct ProcessWorker {
    /// The unique ID of this worker, used for pinning jobs.
    pub id: usize,

    /// The queue owned by this worker.
    queue: RcQueue<ServerPointer>,

    /// The state of the pool this worker belongs to.
    state: ArcWithoutWeak<PoolState>,

    /// The mode this worker is in.
    mode: Mode,

    /// The allocator to use when running a process in this thread.
    allocator: BumpAllocator,
}

impl ProcessWorker {
    /// Starts the main process worker and schedules the main process onto it.
    ///
    /// The main worker is an exclusive worker, and only runs the main process.
    pub fn main(
        queue: RcQueue<ServerPointer>,
        state: ArcWithoutWeak<PoolState>,
        vm_state: &VmState,
        method: MethodPointer,
    ) -> Self {
        let mut allocator =
            BumpAllocator::new(vm_state.permanent_space.blocks.clone());
        let server = Server::main(
            &mut allocator,
            vm_state.permanent_space.main_process_class(),
            vm_state.permanent_space.generator_class(),
            method,
        );

        queue.push_internal(server);

        ProcessWorker {
            id: 0,
            queue,
            state,
            allocator,
            mode: Mode::Exclusive,
        }
    }

    /// Starts a new worker operating in the normal mode.
    pub fn new(
        id: usize,
        queue: RcQueue<ServerPointer>,
        state: ArcWithoutWeak<PoolState>,
        vm_state: &VmState,
    ) -> Self {
        let allocator =
            BumpAllocator::new(vm_state.permanent_space.blocks.clone());

        ProcessWorker {
            id,
            queue,
            state,
            allocator,
            mode: Mode::Normal,
        }
    }

    pub fn allocator(&mut self) -> &mut BumpAllocator {
        &mut self.allocator
    }

    pub fn run(&mut self, vm_state: &VmState) {
        while self.state.is_alive() {
            match self.mode {
                Mode::Normal => self.normal_iteration(vm_state),
                Mode::Exclusive => self.exclusive_iteration(vm_state),
            };
        }
    }

    /// Changes the worker state so it operates in exclusive mode.
    ///
    /// When in exclusive mode, only the currently running job will be allowed
    /// to run on this worker. All other jobs are pushed back into the global
    /// queue.
    pub fn enter_exclusive_mode(&mut self) {
        self.queue.move_external_jobs();

        while let Some(job) = self.queue.pop() {
            self.state.push_global(job);
        }

        self.mode = Mode::Exclusive;
    }

    pub fn leave_exclusive_mode(&mut self) {
        self.mode = Mode::Normal;
    }

    /// Performs a single iteration of the normal work loop.
    fn normal_iteration(&mut self, vm_state: &VmState) {
        if self.process_local_jobs(vm_state) {
            return;
        }

        if self.steal_from_other_queue() {
            return;
        }

        if self.queue.move_external_jobs() {
            return;
        }

        if self.steal_from_global_queue() {
            return;
        }

        self.state.park_while(|| {
            !self.state.has_global_jobs() && !self.queue.has_external_jobs()
        });
    }

    /// Runs a single iteration of an exclusive work loop.
    fn exclusive_iteration(&mut self, vm_state: &VmState) {
        if self.process_local_jobs(vm_state) {
            return;
        }

        // Moving external jobs would allow other workers to steal them,
        // starving the current worker of pinned jobs. Since only one job can be
        // pinned to a worker, we don't need a loop here.
        if let Some(job) = self.queue.pop_external_job() {
            self.process_job(job, vm_state);
            return;
        }

        self.state.park_while(|| !self.queue.has_external_jobs());
    }

    /// Processes all local jobs until we run out of work.
    ///
    /// This method returns true if the worker should self terminate.
    fn process_local_jobs(&mut self, vm_state: &VmState) -> bool {
        loop {
            if !self.state.is_alive() {
                return true;
            }

            if let Some(job) = self.queue.pop() {
                self.process_job(job, vm_state);
            } else {
                return false;
            }
        }
    }

    fn steal_from_other_queue(&self) -> bool {
        // We may try to steal from our queue, but that's OK because it's empty
        // and none of the below operations are blocking.
        for queue in &self.state.queues {
            if queue.steal_into(&self.queue) {
                return true;
            }
        }

        false
    }

    /// Steals a single job from the global queue.
    ///
    /// This method will return `true` if a job was stolen.
    fn steal_from_global_queue(&self) -> bool {
        if let Some(job) = self.state.pop_global() {
            self.queue.push_internal(job);
            true
        } else {
            false
        }
    }

    fn process_job(&mut self, process: ServerPointer, vm_state: &VmState) {
        self.allocator.recycle_blocks(&vm_state.config);

        Machine::new(vm_state).run(self, process);
    }
}

impl Drop for ProcessWorker {
    fn drop(&mut self) {
        while let Some(server) = self.queue.pop() {
            Server::drop(server);
        }

        while let Some(server) = self.queue.pop_external_job() {
            Server::drop(server);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::process::Server;
    use crate::test::setup;

    fn worker(state: &VmState) -> ProcessWorker {
        let pool_state = state.scheduler.primary_pool.state.clone();
        let queue = pool_state.queues[0].clone();

        ProcessWorker::new(0, queue, pool_state, state)
    }

    #[test]
    fn test_run_global_jobs() {
        let (_alloc, state, proc_wrapper) = setup();
        let mut process = proc_wrapper.take_and_forget();
        let mut worker = worker(&state);

        process.set_main();
        worker.state.push_global(process);
        worker.run(&state);

        assert!(worker.state.pop_global().is_none());
        assert_eq!(worker.state.queues[0].has_local_jobs(), false);
    }

    #[test]
    fn test_run_with_external_jobs() {
        let (_alloc, state, proc_wrapper) = setup();
        let mut process = proc_wrapper.take_and_forget();
        let mut worker = worker(&state);

        process.set_main();
        worker.state.queues[0].push_external(process);
        worker.run(&state);

        assert_eq!(worker.state.queues[0].has_external_jobs(), false);
    }

    #[test]
    fn test_run_steal_then_terminate() {
        let (_alloc, state, proc_wrapper) = setup();
        let mut process = proc_wrapper.take_and_forget();
        let mut worker = worker(&state);

        process.set_main();
        worker.state.queues[1].push_internal(process);
        worker.run(&state);

        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_work_and_steal() {
        let (mut alloc, state, proc_wrapper) = setup();
        let mut worker = worker(&state);
        let mut process1 = proc_wrapper.take_and_forget();
        let process2 = Server::alloc(
            &mut alloc,
            state.permanent_space.main_process_class(),
        );

        process1.set_main();
        worker.queue.push_internal(process2);
        worker.state.queues[1].push_internal(process1);

        // Here the order of work is:
        //
        // 1. Process local job
        // 2. Steal from other queue
        // 3. Terminate
        worker.run(&state);

        assert_eq!(worker.queue.has_local_jobs(), false);
        assert_eq!(worker.state.queues[1].has_local_jobs(), false);
    }

    #[test]
    fn test_run_work_then_terminate_steal_loop() {
        let (mut alloc, state, proc_wrapper) = setup();
        let mut worker = worker(&state);
        let mut process1 = proc_wrapper.take_and_forget();
        let process2 = Server::alloc(
            &mut alloc,
            state.permanent_space.main_process_class(),
        );

        process1.set_main();
        worker.state.queues[0].push_internal(process1);
        worker.state.queues[1].push_internal(process2);
        worker.run(&state);

        assert_eq!(worker.state.queues[0].has_local_jobs(), false);
        assert!(worker.state.queues[1].has_local_jobs());

        Server::drop(process2);
    }

    #[test]
    fn test_run_exclusive_iteration() {
        let (_alloc, state, proc_wrapper) = setup();
        let mut worker = worker(&state);
        let mut process = proc_wrapper.take_and_forget();

        process.set_main();
        worker.enter_exclusive_mode();
        worker.queue.push_external(process);
        worker.run(&state);

        assert_eq!(worker.queue.has_external_jobs(), false);
    }

    #[test]
    fn test_enter_exclusive_mode() {
        let (mut alloc, state, proc_wrapper) = setup();
        let process1 = proc_wrapper.take_and_forget();
        let process2 = Server::alloc(
            &mut alloc,
            state.permanent_space.main_process_class(),
        );
        let mut worker = worker(&state);

        worker.queue.push_internal(process1);
        worker.queue.push_external(process2);
        worker.enter_exclusive_mode();

        assert_eq!(worker.mode, Mode::Exclusive);
        assert_eq!(worker.queue.has_local_jobs(), false);
        assert!(worker.queue.pop_external_job().is_none());
    }

    #[test]
    fn test_leave_exclusive_mode() {
        let (_alloc, state, _process) = setup();
        let mut worker = worker(&state);

        worker.enter_exclusive_mode();
        worker.leave_exclusive_mode();

        assert_eq!(worker.mode, Mode::Normal);
    }
}
