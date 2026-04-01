pub mod constants;
pub mod sock_holder;
pub mod socket_callbacks;
/// Shared stack code for both sync and async clients
pub mod stack_error;

use crate::socket::Socket;
use socket_callbacks::{ClientSocketOp, Handle};

use crate::manager::{Manager, SocketOptions, TcpSockOpts, UdpSockOpts};
#[cfg(feature = "ssl")]
use crate::manager::{SslSockConfig, SslSockOpts};
use crate::transfer::Xfer;
use sock_holder::SocketStore;
use socket_callbacks::SocketCallbacks;

pub use stack_error::StackError;

pub(crate) struct Stack<'a, X: Xfer> {
    manager: &'a mut Manager<X>,
    callbacks: &'a mut SocketCallbacks,
}

impl<'a, X: Xfer> Stack<'a, X> {
    pub(crate) fn new(manager: &'a mut Manager<X>, callbacks: &'a mut SocketCallbacks) -> Self {
        Self { manager, callbacks }
    }
    pub(crate) fn set_socket_option(
        &mut self,
        socket: &Handle,
        option: &SocketOptions,
    ) -> Result<(), StackError> {
        match option {
            SocketOptions::Udp(opts) => {
                let (sock, _) = self
                    .callbacks
                    .udp_sockets
                    .get(*socket)
                    .ok_or(StackError::SocketNotFound)?;

                if let UdpSockOpts::ReceiveTimeout(timeout) = opts {
                    // Receive timeout are handled by winc stack not by module.
                    sock.set_recv_timeout(*timeout);
                } else {
                    self.manager.send_setsockopt(*sock, opts)?;
                }
            }

            SocketOptions::Tcp(opts) => {
                let (sock, _) = self
                    .callbacks
                    .tcp_sockets
                    .get(*socket)
                    .ok_or(StackError::SocketNotFound)?;

                match opts {
                    #[cfg(feature = "ssl")]
                    TcpSockOpts::Ssl(ssl_opts) => {
                        match *ssl_opts {
                            SslSockOpts::SetSni(_) => {
                                self.manager.send_ssl_setsockopt(*sock, ssl_opts)?;
                            }
                            SslSockOpts::Config(cfg, en) => {
                                if cfg == SslSockConfig::EnableSSL && en {
                                    if (sock.get_ssl_cfg() & u8::from(cfg)) == cfg.into() {
                                        return Ok(());
                                    } else {
                                        self.manager.send_ssl_sock_create(*sock)?;
                                    }
                                }
                                // Set the SSL flags
                                sock.set_ssl_cfg(cfg.into(), en);
                            }
                        }
                    }
                    TcpSockOpts::ReceiveTimeout(timeout) => {
                        // Receive timeout are handled by winc stack not by module.
                        sock.set_recv_timeout(*timeout);
                    }
                }
            }
        }

        Ok(())
    }
}
