//! Scheduling and execution of lightweight Inko processes.
use crate::mem::process::ServerPointer;
use crate::scheduler::process_pool::ProcessPool;

/// A ProcessScheduler handles the execution of processes.
///
/// A ProcessScheduler consists out of two pools: a primary pool, and a blocking
/// pool. The primary pool is used for executing all processes by default.
/// Processes may be moved to the blocking pool (and back) whenever they need to
/// perform a blocking operation, such as reading from a file.
pub struct ProcessScheduler {
    /// The pool to use for executing most processes.
    pub primary_pool: ProcessPool,

    /// The pool to use for executing processes that perform blocking
    /// operations.
    pub blocking_pool: ProcessPool,
}

impl ProcessScheduler {
    /// Creates a new ProcessScheduler with the given number of primary and
    /// blocking threads.
    pub fn new(primary: u16, blocking: u16) -> Self {
        ProcessScheduler {
            // The primary pool gets one extra thread, as the main thread is
            // reserved for the main process. This makes interfacing with C
            // easier, as the main process is guaranteed to always run on the
            // same OS thread.
            primary_pool: ProcessPool::new("primary".to_string(), primary + 1),
            blocking_pool: ProcessPool::new("blocking".to_string(), blocking),
        }
    }

    /// Informs the scheduler it needs to terminate as soon as possible.
    pub fn terminate(&self) {
        self.primary_pool.terminate();
        self.blocking_pool.terminate();
    }

    /// Schedules a process in one of the pools.
    pub fn schedule(&self, process: ServerPointer) {
        let pool = if process.is_blocking() {
            &self.blocking_pool
        } else {
            &self.primary_pool
        };

        if let Some(thread_id) = process.thread_id() {
            pool.schedule_onto_queue(thread_id, process);
        } else {
            pool.schedule(process);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::setup;

    #[test]
    fn test_terminate() {
        let scheduler = ProcessScheduler::new(1, 1);

        scheduler.terminate();

        assert_eq!(scheduler.primary_pool.state.is_alive(), false);
        assert_eq!(scheduler.blocking_pool.state.is_alive(), false);
    }

    #[test]
    fn test_schedule_on_primary() {
        let scheduler = ProcessScheduler::new(1, 1);
        let (_alloc, _state, process) = setup();
        let proc = *process;

        scheduler.schedule(proc);

        assert!(scheduler.primary_pool.state.pop_global() == Some(proc));
        assert!(scheduler.blocking_pool.state.pop_global().is_none());
    }

    #[test]
    fn test_schedule_on_blocking() {
        let scheduler = ProcessScheduler::new(1, 1);
        let (_alloc, _state, mut process) = setup();
        let proc = *process;

        process.set_blocking();
        scheduler.schedule(proc);

        assert!(scheduler.primary_pool.state.pop_global().is_none());
        assert!(scheduler.blocking_pool.state.pop_global() == Some(proc));
    }

    #[test]
    fn test_schedule_pinned() {
        let scheduler = ProcessScheduler::new(2, 2);
        let (_alloc, _state, mut process) = setup();
        let proc = *process;

        process.pin_to_thread(1);
        scheduler.schedule(proc);

        assert!(scheduler.primary_pool.state.pop_global().is_none());
        assert!(scheduler.blocking_pool.state.pop_global().is_none());
        assert!(scheduler.primary_pool.state.queues[1].has_external_jobs());
    }
}
