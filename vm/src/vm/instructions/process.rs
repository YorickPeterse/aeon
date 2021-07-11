//! VM functions for working with Inko processes.
use crate::duration;
use crate::indexes::MethodIndex;
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{ClassPointer, Float};
use crate::mem::process::{
    Client, ClientPointer, Finished, RescheduleRights, Server, ServerPointer,
};
use crate::scheduler::process_worker::ProcessWorker;
use crate::scheduler::timeouts::Timeout;
use crate::vm::state::State;

#[inline(always)]
pub fn allocate(
    state: &State,
    worker: &mut ProcessWorker,
    generator: GeneratorPointer,
    server_class_ptr: Pointer,
    start_reg: u16,
    num_args: u16,
) -> Pointer {
    let client_class = state.permanent_space.client_class();
    let server_class = unsafe { ClassPointer::new(server_class_ptr) };
    let alloc = worker.allocator();
    let mut server = Server::alloc(alloc, server_class);

    server.set_values(generator.context.get_registers(start_reg, num_args));
    Client::alloc(alloc, client_class, server).as_pointer()
}

#[inline(always)]
pub fn send_message(
    state: &State,
    generator: GeneratorPointer,
    client_ptr: Pointer,
    method: u16,
    start_reg: u16,
    num_args: u16,
) {
    let client = unsafe { ClientPointer::new(client_ptr) };
    let mut server = client.server();
    let args = generator.context.get_registers(start_reg, num_args);

    match server.send_message(unsafe { MethodIndex::new(method) }, args) {
        RescheduleRights::AcquiredWithTimeout => {
            state.timeout_worker.increase_expired_timeouts();
            state.scheduler.schedule(server);
        }
        RescheduleRights::Acquired => {
            state.scheduler.schedule(server);
        }
        _ => {}
    }
}

#[inline(always)]
pub fn yield_control(state: &State, process: ServerPointer) {
    state.scheduler.schedule(process);
}

#[inline(always)]
pub fn suspend(state: &State, mut process: ServerPointer, time_ptr: Pointer) {
    let time = unsafe { Float::read(time_ptr) };
    let timeout = Timeout::with_rc(duration::from_f64(time));

    process.suspend(timeout.clone());
    state.timeout_worker.suspend(process, timeout);
}

#[inline(always)]
pub fn set_blocking(
    state: &State,
    mut server: ServerPointer,
    blocking: Pointer,
) -> Pointer {
    let is_blocking = blocking == state.permanent_space.true_singleton;

    if server.thread_id().is_some() || is_blocking == server.is_blocking() {
        // If a process is pinned, we can't move it to another pool.
        return state.permanent_space.false_singleton;
    }

    if is_blocking {
        server.set_blocking();
    } else {
        server.no_longer_blocking();
    }

    state.permanent_space.true_singleton
}

#[inline(always)]
pub fn set_pinned(
    state: &State,
    worker: &mut ProcessWorker,
    mut server: ServerPointer,
    pinned: Pointer,
) -> Pointer {
    if pinned == state.permanent_space.true_singleton {
        let result = if server.thread_id().is_some() {
            state.permanent_space.false_singleton
        } else {
            server.pin_to_thread(worker.id as u16);
            state.permanent_space.true_singleton
        };

        worker.enter_exclusive_mode();

        return result;
    }

    server.unpin_from_thread();
    worker.leave_exclusive_mode();
    state.permanent_space.false_singleton
}

#[inline(always)]
pub fn finish_generator(
    state: &State,
    worker: &mut ProcessWorker,
    process: ServerPointer,
) {
    match process.finish_generator() {
        Finished::Reschedule => state.scheduler.schedule(process),
        Finished::Terminate => {
            if process.thread_id().is_some() {
                worker.leave_exclusive_mode();
            }

            if process.is_main() {
                state.terminate(0);
            }

            Server::drop(process);
        }
        Finished::WaitForMessage => {
            // When waiting for a message, clients will reschedule or terminate
            // the process when needed. This means at this point we can't use
            // the process anymore, as it may have already been rescheduled.
        }
    }
}
