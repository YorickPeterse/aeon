//! State management of a thread pool.
use crate::mem::process::{Server, ServerPointer};
use crate::scheduler::park_group::ParkGroup;
use crate::scheduler::queue::{Queue, RcQueue};
use crossbeam_deque::{Injector, Steal};
use std::iter;
use std::ops::Drop;
use std::sync::atomic::{AtomicBool, Ordering};

/// The internal state of a single pool, shared between the many workers that
/// belong to the pool.
pub struct PoolState {
    /// The queues available for workers to store work in and steal work from.
    pub queues: Vec<RcQueue<ServerPointer>>,

    /// A boolean indicating if the scheduler is alive, or should shut down.
    alive: AtomicBool,

    /// The global queue on which new jobs will be scheduled,
    global_queue: Injector<ServerPointer>,

    /// Used for parking and unparking worker threads.
    park_group: ParkGroup,
}

impl PoolState {
    /// Creates a new state for the given number worker threads.
    ///
    /// Threads are not started by this method, and instead must be started
    /// manually.
    pub fn new(threads: u16) -> Self {
        let queues = iter::repeat_with(Queue::with_rc)
            .take(threads as usize)
            .collect();

        PoolState {
            alive: AtomicBool::new(true),
            queues,
            global_queue: Injector::new(),
            park_group: ParkGroup::new(),
        }
    }

    /// Schedules a new job onto the global queue.
    pub fn push_global(&self, value: ServerPointer) {
        self.global_queue.push(value);
        self.park_group.notify_one();
    }

    /// Schedules a job onto a specific queue.
    ///
    /// This method will panic if the queue index is invalid.
    pub fn schedule_onto_queue(&self, queue: u16, value: ServerPointer) {
        self.queues[queue as usize].push_external(value);

        // A worker might be parked when sending it an external message, so we
        // have to wake them up. We have to notify all workers instead of a
        // single one, otherwise we may end up notifying a different worker.
        self.park_group.notify_all();
    }

    /// Pops a value off the global queue.
    ///
    /// This method will block the calling thread until a value is available.
    pub fn pop_global(&self) -> Option<ServerPointer> {
        loop {
            match self.global_queue.steal() {
                Steal::Empty => {
                    return None;
                }
                Steal::Retry => {}
                Steal::Success(value) => {
                    return Some(value);
                }
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub fn terminate(&self) {
        self.alive.store(false, Ordering::Release);
        self.notify_all();
    }

    pub fn notify_all(&self) {
        self.park_group.notify_all();
    }

    /// Parks the current thread as long as the given condition is true.
    pub fn park_while<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        self.park_group
            .park_while(|| self.is_alive() && condition());
    }

    /// Returns true if one or more jobs are present in the global queue.
    pub fn has_global_jobs(&self) -> bool {
        !self.global_queue.is_empty()
    }
}

impl Drop for PoolState {
    fn drop(&mut self) {
        while let Some(server) = self.global_queue.steal().success() {
            Server::drop(server);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc_without_weak::ArcWithoutWeak;
    use crate::test::setup;
    use std::mem;
    use std::thread;

    #[test]
    fn test_memory_size() {
        assert_eq!(mem::size_of::<PoolState>(), 384);
    }

    #[test]
    fn test_new() {
        let state: PoolState = PoolState::new(4);

        assert_eq!(state.queues.len(), 4);
    }

    #[test]
    fn test_push_global() {
        let (_alloc, _state, proc_wrapper) = setup();
        let process = proc_wrapper.take_and_forget();
        let state = PoolState::new(1);

        state.push_global(process);

        assert_eq!(state.global_queue.is_empty(), false);
    }

    #[test]
    fn test_pop_global() {
        let (_alloc, _state, process) = setup();
        let state = PoolState::new(1);

        state.push_global(*process);

        assert!(state.pop_global().is_some());
        assert!(state.pop_global().is_none());
    }

    #[test]
    fn test_terminate() {
        let state: PoolState = PoolState::new(4);

        assert!(state.is_alive());

        state.terminate();

        assert_eq!(state.is_alive(), false);
    }

    #[test]
    fn test_park_while() {
        let state: PoolState = PoolState::new(4);
        let mut number = 0;

        state.park_while(|| false);

        number += 1;

        state.terminate();
        state.park_while(|| true);

        number += 1;

        assert_eq!(number, 2);
    }

    #[test]
    fn test_has_global_jobs() {
        let (_alloc, _state, proc_wrapper) = setup();
        let process = proc_wrapper.take_and_forget();
        let state = PoolState::new(4);

        assert_eq!(state.has_global_jobs(), false);

        state.push_global(process);

        assert!(state.has_global_jobs());
    }

    #[test]
    fn test_schedule_onto_queue() {
        let (_alloc, _state, process) = setup();
        let state = PoolState::new(1);

        state.schedule_onto_queue(0, *process);

        assert!(state.queues[0].has_external_jobs());
    }

    #[test]
    #[should_panic]
    fn test_schedule_onto_invalid_queue() {
        let (_alloc, _state, process) = setup();
        let state = PoolState::new(1);

        state.schedule_onto_queue(1, *process);
    }

    #[test]
    fn test_schedule_onto_queue_wake_up() {
        let (_alloc, _state, process) = setup();
        let state = ArcWithoutWeak::new(PoolState::new(1));
        let state_clone = state.clone();

        let handle = thread::spawn(move || {
            let queue = &state_clone.queues[0];

            state_clone.park_while(|| !queue.has_external_jobs());

            queue.pop_external_job()
        });

        state.schedule_onto_queue(0, *process);

        let job = handle.join().unwrap();

        assert!(job.is_some());
    }
}
