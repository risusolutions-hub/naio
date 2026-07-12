//! Zero-dependency readiness poller + executor for Niao async I/O.

mod channel;
mod executor;
mod poller;
pub mod socket;
mod tcp;
mod timer;

pub use channel::{channel, Receiver, Sender};
pub use executor::{spawn, Executor};
pub use poller::{Interest, Poller, RawSocket};
pub use socket::{Domain, Protocol, SockAddr, Socket, SocketOption, SocketOptionKind, Type};
pub use tcp::{
    tcp_accept, tcp_connect, tcp_listen, tcp_read, tcp_write, wait_readable, wait_writable,
};
pub use timer::{sleep, TimerQueue};

#[cfg(test)]
mod tests;
