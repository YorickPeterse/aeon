//! Rescheduling of processes with expired timeouts.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::mem::process::ServerPointer;
use crate::scheduler::process_scheduler::ProcessScheduler;
use crate::scheduler::timeouts::{Timeout, Timeouts};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// The percentage of timeouts (from 0.0 to 1.0) that can be invalidated before
/// the timeouts heap must be cleaned up.
const FRAGMENTATION_THRESHOLD: f64 = 0.1;

/// The maximum number of messages to process in a single timeout iteration.
const MAX_MESSAGES_PER_ITERATION: usize = 64;

enum Message {
    Suspend(ServerPointer, ArcWithoutWeak<Timeout>),
    Terminate,
}

struct Inner {
    /// The processes suspended with timeouts.
    timeouts: Timeouts,

    /// The receiving half of the channel used for suspending processes.
    receiver: Receiver<Message>,

    /// Indicates if the timeout worker should run or terminate.
    alive: bool,
}

/// A TimeoutWorker is tasked with rescheduling processes when their timeouts
/// expire.
///
/// Processes are suspended by sending messages via a channel, removing the need
/// for heavyweight locking.
///
/// ## Cleaning up of invalid timeouts
///
/// Processes will reschedule other processes when they send a message, and the
/// receiving process is suspended. When a timeout is used, the sending process
/// will invalidate it. This can result in the internal list of timeouts
/// building up lots of invalidated timeouts over time, depending on the
/// expiration time of timeouts that preceed these invalidated timeouts.
///
/// To resolve this, the internal list of timeouts is cleaned up periodically.
/// This is done by the timeout worker itself, not by processes sending
/// messages.  This ensures this cleanup work does not impact threads running
/// processes.
pub struct TimeoutWorker {
    /// The inner part of the rescheduler that can only be used by the thread
    /// that reschedules processes.
    inner: UnsafeCell<Inner>,

    /// The sending half of the channel used for suspending processes.
    sender: Sender<Message>,

    /// The number of timeouts that have been invalidated by sending a message
    /// to the process, before the timeout expired.
    expired: AtomicUsize,
}

unsafe impl Sync for TimeoutWorker {}

impl TimeoutWorker {
    /// Creates a new TimeoutWorker.
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();

        let inner = Inner {
            timeouts: Timeouts::new(),
            receiver,
            alive: true,
        };

        TimeoutWorker {
            inner: UnsafeCell::new(inner),
            expired: AtomicUsize::new(0),
            sender,
        }
    }

    pub fn terminate(&self) {
        self.sender
            .send(Message::Terminate)
            .expect("Failed to terminate because the channel was closed");
    }

    pub fn increase_expired_timeouts(&self) {
        self.expired.fetch_add(1, Ordering::AcqRel);
    }

    pub fn run(&self, scheduler: &ProcessScheduler) {
        while self.is_alive() {
            self.defragment_heap();
            self.handle_pending_messages();

            // This ensures we don't end up waiting for a message below if we
            // were instructed to terminated when processing the pending
            // messages.
            if !self.is_alive() {
                return;
            }

            let time_until_expiration =
                self.reschedule_expired_processes(scheduler);

            if let Some(duration) = time_until_expiration {
                self.wait_for_message_with_timeout(duration);
            } else {
                // When there are no timeouts there's no point in periodically
                // processing the list of timeouts, so instead we wait until the
                // first one is added.
                self.wait_for_message();
            }
        }
    }

    pub fn suspend(
        &self,
        process: ServerPointer,
        timeout: ArcWithoutWeak<Timeout>,
    ) {
        self.sender
            .send(Message::Suspend(process, timeout))
            .expect("Failed to suspend because the channel was closed");
    }

    fn reschedule_expired_processes(
        &self,
        scheduler: &ProcessScheduler,
    ) -> Option<Duration> {
        let inner = self.inner_mut();
        let (expired, time_until_expiration) =
            inner.timeouts.processes_to_reschedule();

        for process in expired {
            scheduler.schedule(process);
        }

        time_until_expiration
    }

    fn handle_pending_messages(&self) {
        for message in self
            .inner_mut()
            .receiver
            .try_iter()
            .take(MAX_MESSAGES_PER_ITERATION)
        {
            self.handle_message(message);
        }
    }

    fn wait_for_message(&self) {
        let message = self
            .inner()
            .receiver
            .recv()
            .expect("Attempt to receive from a closed channel");

        self.handle_message(message);
    }

    fn wait_for_message_with_timeout(&self, wait_for: Duration) {
        if let Ok(message) = self.inner().receiver.recv_timeout(wait_for) {
            self.handle_message(message);
        }
    }

    fn handle_message(&self, message: Message) {
        let inner = self.inner_mut();

        match message {
            Message::Suspend(process, timeout) => {
                inner.timeouts.insert(process, timeout);
            }
            Message::Terminate => {
                inner.alive = false;
            }
        }
    }

    fn is_alive(&self) -> bool {
        self.inner().alive
    }

    fn number_of_expired_timeouts(&self) -> f64 {
        self.expired.load(Ordering::Acquire) as f64
    }

    fn heap_is_fragmented(&self) -> bool {
        self.number_of_expired_timeouts() / self.inner().timeouts.len() as f64
            >= FRAGMENTATION_THRESHOLD
    }

    fn defragment_heap(&self) {
        if !self.heap_is_fragmented() {
            return;
        }

        let removed = self.inner_mut().timeouts.remove_invalid_entries();

        self.expired.fetch_sub(removed, Ordering::AcqRel);
    }

    fn inner(&self) -> &Inner {
        unsafe { &*self.inner.get() }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(clippy::mut_from_ref))]
    fn inner_mut(&self) -> &mut Inner {
        unsafe { &mut *self.inner.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc_without_weak::ArcWithoutWeak;
    use crate::scheduler::process_scheduler::ProcessScheduler;
    use crate::test::setup;
    use std::thread;
    use std::time::Instant;

    #[test]
    fn test_new() {
        let worker = TimeoutWorker::new();

        assert!(worker.inner().alive);
        assert_eq!(worker.inner().timeouts.len(), 0);
        assert_eq!(worker.expired.load(Ordering::Acquire), 0);
    }

    #[test]
    fn test_suspend() {
        let worker = TimeoutWorker::new();
        let (_alloc, _state, process) = setup();

        worker.suspend(*process, Timeout::with_rc(Duration::from_secs(1)));

        assert!(worker.inner().receiver.recv().is_ok());
    }

    #[test]
    fn test_terminate() {
        let worker = TimeoutWorker::new();

        worker.terminate();

        assert!(worker.inner().receiver.recv().is_ok());
    }

    #[test]
    fn test_increase_expired_timeouts() {
        let worker = TimeoutWorker::new();

        worker.increase_expired_timeouts();

        assert_eq!(worker.expired.load(Ordering::Acquire), 1);
    }

    #[test]
    fn test_run_with_fragmented_heap() {
        let (_alloc, _state, proc_wrapper) = setup();
        let worker = TimeoutWorker::new();
        let scheduler = ProcessScheduler::new(1, 1);
        let process = proc_wrapper.take_and_forget();

        for time in &[10_u64, 5_u64] {
            let timeout = Timeout::with_rc(Duration::from_secs(*time));

            process.state().waiting_for_future(Some(timeout.clone()));
            worker.suspend(process, timeout);
        }

        worker.increase_expired_timeouts();

        // This makes sure the timeouts are present before we start the run
        // loop.
        worker.wait_for_message();
        worker.wait_for_message();
        worker.terminate();

        worker.run(&scheduler);

        assert_eq!(worker.inner().timeouts.len(), 1);
        assert_eq!(worker.expired.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_run_with_message() {
        let (_alloc, _state, proc_wrapper) = setup();
        let process = proc_wrapper.take_and_forget();
        let worker = TimeoutWorker::new();
        let scheduler = ProcessScheduler::new(1, 1);
        let timeout = Timeout::with_rc(Duration::from_secs(10));

        process.state().waiting_for_future(Some(timeout.clone()));
        worker.suspend(process, timeout);
        worker.terminate();
        worker.run(&scheduler);

        assert_eq!(worker.inner().timeouts.len(), 1);
    }

    #[test]
    fn test_run_with_reschedule() {
        let (_alloc, _state, process) = setup();
        let worker = ArcWithoutWeak::new(TimeoutWorker::new());
        let scheduler = ArcWithoutWeak::new(ProcessScheduler::new(1, 1));
        let timeout = Timeout::with_rc(Duration::from_millis(50));

        process.state().waiting_for_future(Some(timeout.clone()));
        worker.suspend(*process, timeout);

        let handle = {
            let worker_clone = worker.clone();
            let scheduler_clone = scheduler.clone();

            thread::spawn(move || {
                let start = Instant::now();

                worker_clone.run(&scheduler_clone);

                start.elapsed()
            })
        };

        let start = Instant::now();
        let mut rescheduled = None;

        while start.elapsed() <= Duration::from_secs(5) && rescheduled.is_none()
        {
            rescheduled = scheduler.primary_pool.state.pop_global();

            thread::sleep(Duration::from_millis(5));
        }

        worker.terminate();

        let duration =
            handle.join().expect("Failed to join the timeout worker");

        assert!(rescheduled.is_some());
        assert!(duration >= Duration::from_millis(50));
        assert_eq!((*worker).inner().timeouts.len(), 0);
    }

    #[test]
    fn test_reschedule_expired_processes_with_expired_process() {
        let (_alloc, _state, process) = setup();
        let worker = TimeoutWorker::new();
        let scheduler = ProcessScheduler::new(1, 1);
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        process.state().waiting_for_future(Some(timeout.clone()));
        worker.suspend(*process, timeout);
        worker.wait_for_message();
        worker.reschedule_expired_processes(&scheduler);

        assert!(scheduler.primary_pool.state.pop_global().is_some());
    }

    #[test]
    fn test_reschedule_expired_processes_without_expired_process() {
        let (_alloc, _state, proc_wrapper) = setup();
        let process = proc_wrapper.take_and_forget();
        let worker = TimeoutWorker::new();
        let scheduler = ProcessScheduler::new(1, 1);
        let timeout = Timeout::with_rc(Duration::from_secs(5));

        process.state().waiting_for_future(Some(timeout.clone()));
        worker.suspend(process, timeout);
        worker.wait_for_message();
        worker.reschedule_expired_processes(&scheduler);

        assert!(scheduler.primary_pool.state.pop_global().is_none());
    }

    #[test]
    fn test_handle_pending_messages() {
        let worker = TimeoutWorker::new();

        worker.terminate();
        worker.handle_pending_messages();

        assert_eq!(worker.is_alive(), false);
    }

    #[test]
    fn test_handle_pending_messages_with_many_messages() {
        let worker = TimeoutWorker::new();

        for _ in 0..(MAX_MESSAGES_PER_ITERATION + 1) {
            worker.terminate();
        }

        worker.handle_pending_messages();

        assert!(worker.inner().receiver.recv().is_ok());
    }

    #[test]
    fn test_wait_for_message() {
        let worker = TimeoutWorker::new();

        worker.terminate();
        worker.wait_for_message();

        assert_eq!(worker.is_alive(), false);
    }

    #[test]
    fn test_wait_for_message_with_timeout_with_message() {
        let worker = TimeoutWorker::new();

        worker.terminate();
        worker.wait_for_message_with_timeout(Duration::from_millis(5));

        assert_eq!(worker.is_alive(), false);
    }

    #[test]
    fn test_wait_for_message_with_timeout_without_message() {
        let worker = TimeoutWorker::new();
        let start = Instant::now();

        worker.wait_for_message_with_timeout(Duration::from_millis(10));

        assert!(start.elapsed() >= Duration::from_millis(9));
    }

    #[test]
    fn test_handle_message() {
        let worker = TimeoutWorker::new();

        worker.handle_message(Message::Terminate);

        assert_eq!(worker.is_alive(), false);
    }

    #[test]
    fn test_is_alive() {
        let worker = TimeoutWorker::new();

        assert!(worker.is_alive());

        worker.handle_message(Message::Terminate);

        assert_eq!(worker.is_alive(), false);
    }

    #[test]
    fn test_number_of_expired_timeouts() {
        let worker = TimeoutWorker::new();

        assert_eq!(worker.number_of_expired_timeouts(), 0.0);

        worker.increase_expired_timeouts();

        assert_eq!(worker.number_of_expired_timeouts(), 1.0);
    }

    #[test]
    fn test_heap_is_fragmented() {
        let (_alloc, _state, proc_wrapper) = setup();
        let process = proc_wrapper.take_and_forget();
        let worker = TimeoutWorker::new();

        assert_eq!(worker.heap_is_fragmented(), false);

        for time in &[1_u64, 2_u64] {
            let timeout = Timeout::with_rc(Duration::from_secs(*time));

            process.state().waiting_for_future(Some(timeout.clone()));
            worker.suspend(process, timeout);
        }

        worker.increase_expired_timeouts();
        worker.wait_for_message();
        worker.wait_for_message();

        assert!(worker.heap_is_fragmented());
    }

    #[test]
    fn test_defragment_heap_without_fragmentation() {
        let (_alloc, _state, proc_wrapper) = setup();
        let process = proc_wrapper.take_and_forget();
        let worker = TimeoutWorker::new();
        let timeout = Timeout::with_rc(Duration::from_secs(1));

        process.state().waiting_for_future(Some(timeout.clone()));
        worker.suspend(process, timeout);
        worker.wait_for_message();
        worker.defragment_heap();

        assert_eq!(worker.expired.load(Ordering::Acquire), 0);
        assert_eq!(worker.inner().timeouts.len(), 1);
    }

    #[test]
    fn test_defragment_heap_with_fragmentation() {
        let (_alloc, _state, proc_wrapper) = setup();
        let process = proc_wrapper.take_and_forget();
        let worker = TimeoutWorker::new();

        for time in &[1_u64, 1_u64] {
            let timeout = Timeout::with_rc(Duration::from_secs(*time));

            process.state().waiting_for_future(Some(timeout.clone()));
            worker.suspend(process, timeout);
        }

        worker.increase_expired_timeouts();
        worker.wait_for_message();
        worker.wait_for_message();
        worker.defragment_heap();

        assert_eq!(worker.expired.load(Ordering::Acquire), 0);
        assert_eq!(worker.inner().timeouts.len(), 1);
    }
}
