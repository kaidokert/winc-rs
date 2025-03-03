use crate::manager::Manager;
use crate::manager::{PingError, ScanResult, SOCKET_BUFFER_MAX_LENGTH};
use crate::socket::Socket;
use crate::transfer::Xfer;

use crate::Ipv4AddrFormatWrapper;

use crate::manager::SocketError;

use crate::{debug, error, info};

/// Opaque handle to a socket. Returned by socket APIs
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Handle(u8);

mod dns;
mod stack_error;
mod tcp_stack;
mod udp_stack;
mod wifi_module;
pub use stack_error::StackError;

mod sock_holder;
use sock_holder::SockHolder;

mod socket_callbacks;
pub use socket_callbacks::PingResult;
use socket_callbacks::SocketCallbacks;

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClientSocketOp {
    None,
    New,
    Connect,
    Send(i16),
    SendTo(i16),
    Recv,
    RecvFrom,
    Bind,
    Listen,
    Accept,
}

#[derive(PartialEq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GlobalOp {
    GetHostByName,
    #[allow(dead_code)] // todo: we'll add this later
    Ping,
}

pub enum GenResult {
    Ip(core::net::Ipv4Addr),
    Len(usize),
    Accept(core::net::SocketAddrV4, Socket),
}

/// Client for the WincWifi chip.
///
/// This manages the state of the chip and
/// network connections
pub struct WincClient<'a, X: Xfer> {
    manager: Manager<X>,
    delay: &'a mut dyn FnMut(u32),
    recv_timeout: u32,
    poll_loop_delay: u32,
    callbacks: SocketCallbacks,
    next_session_id: u16,
    // TODO: Lets change that per socket
    last_send_addr: Option<core::net::SocketAddrV4>,
    boot: Option<crate::manager::BootState>,
    operation_countdown: u32,
    #[cfg(test)]
    debug_callback: Option<&'a mut dyn FnMut(&mut SocketCallbacks)>,
}

impl<'a, X: Xfer> WincClient<'a, X> {
    // Max send frame length
    const MAX_SEND_LENGTH: usize = 1400;

    const TCP_SOCKET_BACKLOG: u8 = 4;
    const LISTEN_TIMEOUT: u32 = 100;
    const ACCEPT_TIMEOUT: u32 = 100;
    const BIND_TIMEOUT: u32 = 100;
    const SEND_TIMEOUT: u32 = 1000;
    const RECV_TIMEOUT: u32 = 1000;
    const CONNECT_TIMEOUT: u32 = 1000;
    const DNS_TIMEOUT: u32 = 1000;
    const POLL_LOOP_DELAY: u32 = 10;
    /// Create a new WincClient..
    ///
    /// # Arguments
    ///
    /// * `transfer` - The transfer implementation to use for client,
    ///             typically a struct wrapping SPI communication.
    /// * `delay` - A delay function. Currently required - a closure
    ///             that takes millisconds as an arg.
    ///
    ///  See [Xfer] for details how to implement a transfer struct.
    pub fn new(transfer: X, delay: &'a mut impl FnMut(u32)) -> Self {
        let manager = Manager::from_xfer(transfer);
        Self::new_internal(manager, delay)
    }
    fn new_internal(manager: Manager<X>, delay: &'a mut impl FnMut(u32)) -> Self {
        Self {
            manager,
            callbacks: SocketCallbacks::new(),
            delay,
            recv_timeout: Self::RECV_TIMEOUT,
            poll_loop_delay: Self::POLL_LOOP_DELAY,
            next_session_id: 0,
            last_send_addr: None,
            boot: None,
            operation_countdown: 0,
            #[cfg(test)]
            debug_callback: None,
        }
    }
    fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }
    fn dispatch_events(&mut self) -> Result<(), StackError> {
        #[cfg(test)]
        if let Some(callback) = &mut self.debug_callback {
            callback(&mut self.callbacks);
        }
        self.manager
            .dispatch_events_new(&mut self.callbacks)
            .map_err(StackError::DispatchError)
    }
    fn wait_with_timeout<F, T>(
        &mut self,
        timeout: u32,
        mut check_complete: F,
    ) -> Result<T, StackError>
    where
        F: FnMut(&mut Self, u32) -> Option<Result<T, StackError>>,
    {
        self.dispatch_events()?;
        let mut timeout = timeout as i32;
        let mut elapsed = 0;

        loop {
            if timeout <= 0 {
                return Err(StackError::GeneralTimeout);
            }

            if let Some(result) = check_complete(self, elapsed) {
                return result;
            }

            (self.delay)(self.poll_loop_delay);
            self.dispatch_events()?;
            timeout -= self.poll_loop_delay as i32;
            elapsed += self.poll_loop_delay;
        }
    }

    fn wait_for_gen_ack(
        &mut self,
        expect_op: GlobalOp,
        timeout: u32,
    ) -> Result<GenResult, StackError> {
        // Lets clear state
        self.callbacks.last_recv_addr = None;
        self.callbacks.last_error = SocketError::NoError;

        debug!("===>Waiting for gen ack for {:?}", expect_op);

        self.wait_with_timeout(timeout, |client, elapsed| {
            if client.callbacks.global_op.is_none() {
                debug!("<===Ack received {:?} elapsed:{}ms", expect_op, elapsed);

                if let Some(addr) = client.callbacks.last_recv_addr {
                    return Some(Ok(GenResult::Ip(*addr.ip())));
                }

                if client.callbacks.last_error != SocketError::NoError {
                    return Some(Err(StackError::OpFailed(client.callbacks.last_error)));
                }

                return Some(Err(StackError::GlobalOpFailed));
            }
            None
        })
        .map_err(|e| {
            if matches!(e, StackError::GeneralTimeout) {
                match expect_op {
                    GlobalOp::GetHostByName => StackError::DnsTimeout,
                    _ => StackError::GeneralTimeout,
                }
            } else {
                e
            }
        })
    }

    fn wait_for_op_ack(
        &mut self,
        handle: Handle,
        expect_op: ClientSocketOp,
        timeout: u32,
        tcp: bool,
    ) -> Result<GenResult, StackError> {
        self.callbacks.last_recv_addr = None;
        self.callbacks.last_error = SocketError::NoError;

        debug!("===>Waiting for op ack for {:?}", expect_op);

        self.wait_with_timeout(timeout, |client, elapsed| {
            let (_sock, op) = match tcp {
                true => client.callbacks.tcp_sockets.get(handle).unwrap(),
                false => client.callbacks.udp_sockets.get(handle).unwrap(),
            };

            if *op == ClientSocketOp::None {
                debug!(
                    "<===Ack received for {:?}, recv_len:{:?}, elapsed:{}ms",
                    expect_op, client.callbacks.recv_len, elapsed
                );

                if let Some(accepted_socket) = client.callbacks.last_accepted_socket.take() {
                    return Some(Ok(GenResult::Accept(
                        client.callbacks.last_recv_addr.unwrap(),
                        accepted_socket,
                    )));
                }

                if client.callbacks.last_error != SocketError::NoError {
                    return Some(Err(StackError::OpFailed(client.callbacks.last_error)));
                }

                return Some(Ok(GenResult::Len(client.callbacks.recv_len)));
            }
            None
        })
        .map_err(|e| {
            if matches!(e, StackError::GeneralTimeout) {
                match expect_op {
                    ClientSocketOp::Connect => StackError::ConnectTimeout,
                    ClientSocketOp::Send(_) => StackError::SendTimeout,
                    ClientSocketOp::Recv => StackError::RecvTimeout,
                    _ => StackError::GeneralTimeout,
                }
            } else {
                e
            }
        })
    }
}

#[cfg(test)]
mod test_shared {
    use super::*;

    pub(crate) struct MockTransfer {}

    impl Default for MockTransfer {
        fn default() -> Self {
            Self {}
        }
    }

    impl Xfer for MockTransfer {
        fn recv(&mut self, _: &mut [u8]) -> Result<(), crate::errors::Error> {
            Ok(())
        }
        fn send(&mut self, _: &[u8]) -> Result<(), crate::errors::Error> {
            Ok(())
        }
    }

    pub(crate) fn make_test_client(delay: &mut impl FnMut(u32)) -> WincClient<MockTransfer> {
        let mut client = WincClient::new(MockTransfer::default(), delay);
        client.manager.set_unit_test_mode();
        client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_winc_client() {}

    #[test]
    fn test_fa_client() {}

    #[test]
    fn test_containers() {
        let mut socks = SockHolder::<2, 7>::new();
        let handle0 = socks.add(13).unwrap();
        let (s, _) = socks.get(handle0).unwrap();
        assert_eq!(s.v, 7);
        assert_eq!(s.s, 13);
        let handle1 = socks.add(42).unwrap();
        let (s, _) = socks.get(handle1).unwrap();
        assert_eq!(s.v, 8);
        assert_eq!(s.s, 42);
        assert_eq!(socks.add(42), None);
        socks.remove(handle0);
        let handle2 = socks.add(50).unwrap();
        let (s, _) = socks.get(handle2).unwrap();
        assert_eq!(s.v, 7);
        assert_eq!(s.s, 50);
    }
    #[test]
    fn test_mixmatch() {
        let mut tcp_sockets: SockHolder<7, 0> = SockHolder::new();
        let mut udp_sockets: SockHolder<4, 7> = SockHolder::new();
        let tcp_sock = tcp_sockets.add(13).unwrap();
        assert_eq!(tcp_sock.0, 0);
        assert_eq!(tcp_sockets.get(tcp_sock).unwrap().0.v, 0);
        let udp_sock = udp_sockets.add(42).unwrap();
        assert_eq!(udp_sock.0, 0);
        assert_eq!(udp_sockets.get(udp_sock).unwrap().0.v, 7);
    }
}
