//! Thread pool for executing lightweight Inko processes.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::mem::objects::MethodPointer;
use crate::mem::process::ServerPointer;
use crate::scheduler::join_list::JoinList;
use crate::scheduler::pool_state::PoolState;
use crate::scheduler::process_worker::ProcessWorker;
use crate::scheduler::queue::RcQueue;
use crate::vm::state::RcState as VmState;
use std::thread;

/// A pool of threads for running lightweight processes.
///
/// A pool consists out of one or more workers, each backed by an OS thread.
/// Workers can perform work on their own as well as steal work from other
/// workers.
pub struct ProcessPool {
    pub state: ArcWithoutWeak<PoolState>,

    /// The base name of every thread in this pool.
    name: String,
}

impl ProcessPool {
    pub fn new(name: String, threads: u16) -> Self {
        assert!(
            threads > 0,
            "A ProcessPool requires at least a single thread"
        );

        Self {
            name,
            state: ArcWithoutWeak::new(PoolState::new(threads)),
        }
    }

    /// Schedules a job onto a specific queue.
    pub fn schedule_onto_queue(&self, queue: u16, job: ServerPointer) {
        self.state.schedule_onto_queue(queue, job);
    }

    /// Schedules a job onto the global queue.
    pub fn schedule(&self, job: ServerPointer) {
        self.state.push_global(job);
    }

    /// Informs this pool it should terminate as soon as possible.
    pub fn terminate(&self) {
        self.state.terminate();
    }

    /// Starts the pool, blocking the current thread until the pool is
    /// terminated.
    ///
    /// The current thread will be used to perform jobs scheduled onto the first
    /// queue.
    pub fn start_main(
        &self,
        vm_state: VmState,
        method: MethodPointer,
    ) -> JoinList<()> {
        let join_list = self.spawn_threads_for_range(1, vm_state.clone());
        let queue = self.state.queues[0].clone();
        let mut worker =
            ProcessWorker::main(queue, self.state.clone(), &vm_state, method);

        worker.run(&vm_state);
        join_list
    }

    /// Starts the pool, without blocking the calling thread.
    pub fn start(&self, vm_state: VmState) -> JoinList<()> {
        self.spawn_threads_for_range(0, vm_state)
    }

    /// Spawns OS threads for a range of queues, starting at the given position.
    fn spawn_threads_for_range(
        &self,
        start_at: usize,
        vm_state: VmState,
    ) -> JoinList<()> {
        let mut handles = Vec::new();

        for index in start_at..self.state.queues.len() {
            let handle = self.spawn_thread(
                index,
                vm_state.clone(),
                self.state.queues[index].clone(),
            );

            handles.push(handle);
        }

        JoinList::new(handles)
    }

    fn spawn_thread(
        &self,
        id: usize,
        vm_state: VmState,
        queue: RcQueue<ServerPointer>,
    ) -> thread::JoinHandle<()> {
        let state = self.state.clone();

        thread::Builder::new()
            .name(format!("{} {}", self.name, id))
            .spawn(move || {
                ProcessWorker::new(id, queue, state, &vm_state).run(&vm_state);
            })
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::objects::Method;
    use crate::test::{empty_class, empty_method, empty_module, setup};

    #[test]
    #[should_panic]
    fn test_new_with_zero_threads() {
        ProcessPool::new("test".to_string(), 0);
    }

    #[test]
    fn test_terminate() {
        let (_alloc, state, _process) = setup();
        let pool = &state.scheduler.primary_pool;

        assert_eq!(pool.state.is_alive(), true);
        pool.terminate();
        assert_eq!(pool.state.is_alive(), false);
    }

    #[test]
    fn test_start_main() {
        let (mut alloc, state, _process) = setup();
        let pool = &state.scheduler.primary_pool;
        let class = empty_class("A");
        let module = empty_module(&mut alloc, class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let threads = pool.start_main(state.clone(), method);

        threads.join().unwrap();

        assert_eq!(pool.state.is_alive(), false);

        Method::drop(method);
    }

    #[test]
    fn test_schedule_onto_queue() {
        let (_alloc, state, process) = setup();
        let pool = &state.scheduler.primary_pool;

        pool.schedule_onto_queue(0, *process);

        assert!(pool.state.queues[0].has_external_jobs());
    }

    #[test]
    fn test_spawn_thread() {
        let (_alloc, state, process) = setup();
        let mut proc = process.take_and_forget();

        proc.set_main();
        proc.pin_to_thread(0);

        let pool = &state.scheduler.primary_pool;
        let thread =
            pool.spawn_thread(0, state.clone(), pool.state.queues[0].clone());

        pool.schedule(proc);

        thread.join().unwrap();

        assert_eq!(pool.state.has_global_jobs(), false);
    }
}
