//! Functions for working with non-blocking sockets.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{
    Array, ByteArray, Float, Int, String as InkoString, UnsignedInt,
};
use crate::mem::process::ServerPointer;
use crate::network_poller::Interest;
use crate::runtime_error::RuntimeError;
use crate::socket::Socket;
use crate::vm::state::State;
use std::io::Write;

macro_rules! ret {
    ($result:expr, $state:expr, $proc:expr, $sock:expr, $interest:expr) => {{
        if let Err(ref err) = $result {
            if err.should_poll() {
                $sock.register($proc, &$state.network_poller, $interest)?;
            }
        }

        $result
    }};
}

/// Allocates a new IPv4 socket.
///
/// This function requires requires one argument: the socket type.
pub fn socket_allocate_ipv4(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { UnsignedInt::read(arguments[0]) };
    let socket = Socket::ipv4(kind)?;

    Ok(Pointer::boxed(socket))
}

/// Allocates a new IPv6 socket.
///
/// This function requires requires one argument: the socket type.
pub fn socket_allocate_ipv6(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { UnsignedInt::read(arguments[0]) };
    let socket = Socket::ipv6(kind)?;

    Ok(Pointer::boxed(socket))
}

/// Allocates a new UNIX socket.
///
/// This function requires requires one argument: the socket type.
pub fn socket_allocate_unix(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { UnsignedInt::read(arguments[0]) };
    let socket = Socket::unix(kind)?;

    Ok(Pointer::boxed(socket))
}

/// Writes a String to a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to write to.
/// 2. The String to write.
pub fn socket_write_string(
    state: &State,
    alloc: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let input = unsafe { InkoString::read(&arguments[1]).as_bytes() };
    let res = sock
        .write(input)
        .map(|size| {
            UnsignedInt::alloc(
                alloc,
                state.permanent_space.unsigned_int_class(),
                size as u64,
            )
        })
        .map_err(RuntimeError::from);

    ret!(res, state, process, sock, Interest::Write)
}

/// Writes a ByteArray to a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to write to.
/// 2. The ByteArray to write.
pub fn socket_write_bytes(
    state: &State,
    alloc: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let input = unsafe { arguments[1].get::<ByteArray>() }.value();
    let res = sock
        .write(input)
        .map(|size| {
            UnsignedInt::alloc(
                alloc,
                state.permanent_space.unsigned_int_class(),
                size as u64,
            )
        })
        .map_err(RuntimeError::from);

    ret!(res, state, process, sock, Interest::Write)
}

/// Reads bytes from a socket into a ByteArray.
///
/// This function requires the following arguments:
///
/// 1. The socket to read from.
/// 2. The ByteArray to read into.
/// 3. The number of bytes to read.
pub fn socket_read(
    state: &State,
    alloc: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let amount = unsafe { UnsignedInt::read(arguments[2]) } as usize;

    let result = sock.read(buffer, amount).map(|size| {
        UnsignedInt::alloc(
            alloc,
            state.permanent_space.unsigned_int_class(),
            size as u64,
        )
    });

    ret!(result, state, process, sock, Interest::Read)
}

/// Listens on a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to listen on.
/// 2. The listen backlog.
pub fn socket_listen(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let backlog = unsafe { Int::read(arguments[1]) } as i32;

    sock.listen(backlog)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Binds a socket to an address.
///
/// This function requires the following arguments:
///
/// 1. The socket to bind.
/// 2. The address to bind to.
/// 3. The port to bind to.
pub fn socket_bind(
    state: &State,
    _: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let addr = unsafe { InkoString::read(&arguments[1]) };
    let port = unsafe { UnsignedInt::read(arguments[2]) } as u16;
    let result = sock
        .bind(addr, port)
        .map(|_| state.permanent_space.nil_singleton);

    ret!(result, state, process, sock, Interest::Read)
}

/// Connects a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to connect.
/// 2alloc. The address to connect to.
/// 3. The port to connect to.
pub fn socket_connect(
    state: &State,
    _: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let addr = unsafe { InkoString::read(&arguments[1]) };
    let port = unsafe { UnsignedInt::read(arguments[2]) } as u16;
    let result = sock
        .connect(addr, port)
        .map(|_| state.permanent_space.nil_singleton);

    ret!(result, state, process, sock, Interest::Write)
}

/// Accepts an incoming IPv4/IPv6 connection.
///
/// This function requires one argument: the socket to accept connections on.
pub fn socket_accept_ip(
    state: &State,
    _: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let result = sock.accept().map(Pointer::boxed);

    ret!(result, state, process, sock, Interest::Read)
}

/// Accepts an incoming UNIX connection.
///
/// This function requires one argument: the socket to accept connections on.
pub fn socket_accept_unix(
    state: &State,
    _: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let result = sock.accept().map(Pointer::boxed);

    ret!(result, state, process, sock, Interest::Read)
}

/// Receives data from a socket.
///
/// This function requires the following arguments:
///
/// 1. The socket to receive from.
/// 2. The ByteArray to write into.
/// 3. The number of bytes to read.
pub fn socket_receive_from(
    state: &State,
    alloc: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let amount = unsafe { UnsignedInt::read(arguments[2]) } as usize;
    let result = sock
        .recv_from(buffer, amount)
        .map(|(addr, port)| allocate_address_pair(state, alloc, addr, port));

    ret!(result, state, process, sock, Interest::Read)
}

/// Sends a ByteArray to a socket with a given address.
///
/// This function requires the following arguments:
///
/// 1. The socket to use for sending the data.
/// 2. The ByteArray to send.
/// 3. The address to send the data to.
/// 4. The port to send the data to.
pub fn socket_send_bytes_to(
    state: &State,
    alloc: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let address = unsafe { InkoString::read(&arguments[2]) };
    let port = unsafe { UnsignedInt::read(arguments[3]) } as u16;
    let result = sock.send_to(buffer, address, port).map(|size| {
        UnsignedInt::alloc(
            alloc,
            state.permanent_space.unsigned_int_class(),
            size as u64,
        )
    });

    ret!(result, state, process, sock, Interest::Write)
}

/// Sends a String to a socket with a given address.
///
/// This function requires the following arguments:
///
/// 1. The socket to use for sending the data.
/// 2. The ByteArray to send.
/// 3. The address to send the data to.
/// 4. The port to send the data to.
pub fn socket_send_string_to(
    state: &State,
    alloc: &mut BumpAllocator,
    process: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let buffer = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let address = unsafe { InkoString::read(&arguments[2]) };
    let port = unsafe { UnsignedInt::read(arguments[3]) } as u16;
    let result = sock.send_to(buffer, address, port).map(|size| {
        UnsignedInt::alloc(
            alloc,
            state.permanent_space.unsigned_int_class(),
            size as u64,
        )
    });

    ret!(result, state, process, sock, Interest::Write)
}

/// Shuts down a socket for reading.
///
/// This function requires one argument: the socket to shut down.
pub fn socket_shutdown_read(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.shutdown_read()
        .map(|_| state.permanent_space.nil_singleton)
}

/// Shuts down a socket for writing.
///
/// This function requires one argument: the socket to shut down.
pub fn socket_shutdown_write(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.shutdown_write()
        .map(|_| state.permanent_space.nil_singleton)
}

/// Shuts down a socket for reading and writing.
///
/// This function requires one argument: the socket to shut down.
pub fn socket_shutdown_read_write(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.shutdown_read_write()
        .map(|_| state.permanent_space.nil_singleton)
}

/// Returns the local address of a socket.
///
/// This function requires one argument: the socket to return the address for.
pub fn socket_local_address(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get::<Socket>() };

    sock.local_address()
        .map(|(addr, port)| allocate_address_pair(state, alloc, addr, port))
}

/// Returns the peer address of a socket.
///
/// This function requires one argument: the socket to return the address for.
pub fn socket_peer_address(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get::<Socket>() };

    sock.peer_address()
        .map(|(addr, port)| allocate_address_pair(state, alloc, addr, port))
}

/// Returns the value of the `IP_TTL` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_ttl(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { arguments[0].get::<Socket>() }.ttl()? as u64;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value,
    ))
}

/// Returns the value of the `IPV6_ONLY` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_only_v6(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.only_v6()? {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Returns the value of the `TCP_NODELAY` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_nodelay(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get_mut::<Socket>() }.nodelay()? {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Returns the value of the `SO_BROADCAST` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_broadcast(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.broadcast()? {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Returns the value of the `SO_LINGER` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_linger(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { arguments[0].get::<Socket>() }.linger()?;

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        value,
    ))
}

/// Returns the value of the `SO_RCVBUF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_recv_size(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        unsafe { arguments[0].get::<Socket>() }.recv_buffer_size()? as u64,
    ))
}

/// Returns the value of the `SO_SNDBUF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_send_size(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        unsafe { arguments[0].get::<Socket>() }.send_buffer_size()? as u64,
    ))
}

/// Returns the value of the `SO_KEEPALIVE` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_keepalive(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { arguments[0].get::<Socket>() }.keepalive()?;

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        value,
    ))
}

/// Returns the value of the `IP_MULTICAST_LOOP` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_loop_v4(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.multicast_loop_v4()? {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Returns the value of the `IPV6_MULTICAST_LOOP` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_loop_v6(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.multicast_loop_v6()? {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Returns the value of the `IP_MULTICAST_TTL` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_ttl_v4(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        unsafe { arguments[0].get::<Socket>() }.multicast_ttl_v4()? as u64,
    ))
}

/// Returns the value of the `IPV6_MULTICAST_HOPS` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_hops_v6(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        unsafe { arguments[0].get::<Socket>() }.multicast_hops_v6()? as u64,
    ))
}

/// Returns the value of the `IP_MULTICAST_IF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_if_v4(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { arguments[0].get::<Socket>() }.multicast_if_v4()?;

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        value,
    ))
}

/// Returns the value of the `IPV6_MULTICAST_IF` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_multicast_if_v6(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        unsafe { arguments[0].get::<Socket>() }.multicast_if_v6()? as u64,
    ))
}

/// Returns the value of the `IPV6_UNICAST_HOPS` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_unicast_hops_v6(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        unsafe { arguments[0].get::<Socket>() }.unicast_hops_v6()? as u64,
    ))
}

/// Returns the value of the `SO_REUSEADDR` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_reuse_address(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.reuse_address()? {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Returns the value of the `SO_REUSEPORT` option.
///
/// This function requires one argument: the function to get the value for.
pub fn socket_get_reuse_port(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    if unsafe { arguments[0].get::<Socket>() }.reuse_port()? {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Sets the value of the `IP_TTL` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_ttl(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) } as u32;

    sock.set_ttl(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IPV6_ONLY` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_only_v6(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.set_only_v6(arguments[1] == state.permanent_space.true_singleton)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `TCP_NODELAY` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_nodelay(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.set_nodelay(arguments[1] == state.permanent_space.true_singleton)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `SO_BROADCAST` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_broadcast(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };

    sock.set_broadcast(arguments[1] == state.permanent_space.true_singleton)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `SO_LINGER` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_linger(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { Float::read(arguments[1]) };

    sock.set_linger(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `SO_RCVBUF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_recv_size(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) } as usize;

    sock.set_recv_buffer_size(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `SO_SNDBUF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_send_size(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) } as usize;

    sock.set_send_buffer_size(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `SO_KEEPALIVE` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_keepalive(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { Float::read(arguments[1]) };

    sock.set_keepalive(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IP_MULTICAST_LOOP` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_loop_v4(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = arguments[1];

    sock.set_multicast_loop_v4(value == state.permanent_space.true_singleton)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IPV6_MULTICAST_LOOP` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_loop_v6(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = arguments[1];

    sock.set_multicast_loop_v6(value == state.permanent_space.true_singleton)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IP_MULTICAST_TTL` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_ttl_v4(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) } as u32;

    sock.set_multicast_ttl_v4(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IPV6_MULTICAST_HOPS` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_hops_v6(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) } as u32;

    sock.set_multicast_hops_v6(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IP_MULTICAST_IF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_if_v4(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { InkoString::read(&arguments[1]) };

    sock.set_multicast_if_v4(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IPV6_MULTICAST_IF` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_multicast_if_v6(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) } as u32;

    sock.set_multicast_if_v6(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `IPV6_UNICAST_HOPS` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_unicast_hops_v6(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) } as u32;

    sock.set_unicast_hops_v6(value)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `SO_REUSEADDR` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_reuse_address(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = arguments[1];

    sock.set_reuse_address(value == state.permanent_space.true_singleton)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Sets the value of the `SO_REUSEPORT` option.
///
/// This function requires the following arguments:
///
/// 1. The socket to set the option for.
/// 2. The value to set.
pub fn socket_set_reuse_port(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let sock = unsafe { arguments[0].get_mut::<Socket>() };
    let value = arguments[1];

    sock.set_reuse_port(value == state.permanent_space.true_singleton)?;
    Ok(state.permanent_space.nil_singleton)
}

fn allocate_address_pair(
    state: &State,
    alloc: &mut BumpAllocator,
    addr: String,
    port: u64,
) -> Pointer {
    let addr =
        InkoString::alloc(alloc, state.permanent_space.string_class(), addr);
    let port = UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        port,
    );

    Array::alloc(alloc, state.permanent_space.array_class(), vec![addr, port])
}

register!(
    socket_allocate_ipv4,
    socket_allocate_ipv6,
    socket_allocate_unix,
    socket_write_string,
    socket_write_bytes,
    socket_read,
    socket_listen,
    socket_bind,
    socket_connect,
    socket_accept_ip,
    socket_accept_unix,
    socket_receive_from,
    socket_send_bytes_to,
    socket_send_string_to,
    socket_shutdown_read,
    socket_shutdown_write,
    socket_shutdown_read_write,
    socket_local_address,
    socket_peer_address,
    socket_get_ttl,
    socket_get_only_v6,
    socket_get_nodelay,
    socket_get_broadcast,
    socket_get_linger,
    socket_get_recv_size,
    socket_get_send_size,
    socket_get_keepalive,
    socket_get_multicast_loop_v4,
    socket_get_multicast_loop_v6,
    socket_get_multicast_ttl_v4,
    socket_get_multicast_hops_v6,
    socket_get_multicast_if_v4,
    socket_get_multicast_if_v6,
    socket_get_unicast_hops_v6,
    socket_get_reuse_address,
    socket_get_reuse_port,
    socket_set_ttl,
    socket_set_only_v6,
    socket_set_nodelay,
    socket_set_broadcast,
    socket_set_linger,
    socket_set_recv_size,
    socket_set_send_size,
    socket_set_keepalive,
    socket_set_multicast_loop_v4,
    socket_set_multicast_loop_v6,
    socket_set_multicast_ttl_v4,
    socket_set_multicast_hops_v6,
    socket_set_multicast_if_v4,
    socket_set_multicast_if_v6,
    socket_set_unicast_hops_v6,
    socket_set_reuse_address,
    socket_set_reuse_port
);
