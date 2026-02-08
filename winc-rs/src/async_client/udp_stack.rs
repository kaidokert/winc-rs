use crate::net_ops::op::OpImpl;
use crate::net_ops::udp_receive::UdpReceiveOp;
use crate::net_ops::udp_send::UdpSendOp;
use crate::stack::sock_holder::SocketStore;
use crate::transfer::Xfer;
use crate::Handle;
use crate::StackError;
use embedded_nal_async::{ConnectedUdp, UdpStack, UnconnectedUdp};

use super::{AsyncClient, ClientStack};

/// Connected UDP socket with a fixed remote address.
///
/// Created via [`UdpStack::connect()`] or [`UdpStack::connect_from()`].
/// Implements [`ConnectedUdp`] for send/receive without specifying addresses.
///
/// # Lifetime
/// Holds a reference to the parent AsyncClient for the duration of its use.
///
/// # Drop Behavior
/// The socket is automatically closed when dropped.
pub struct AsyncUdpConnected<'a, X: Xfer> {
    client: &'a AsyncClient<'a, X>,
    socket: Handle,
    remote: core::net::SocketAddrV4,
}

/// UDP socket bound to a unique local address.
///
/// Created via [`UdpStack::bind_single()`].
/// Implements [`UnconnectedUdp`] for receiving from any remote.
///
/// # Lifetime
/// Holds a reference to the parent AsyncClient for the duration of its use.
///
/// # Drop Behavior
/// The socket is automatically closed when dropped.
pub struct AsyncUdpUniquelyBound<'a, X: Xfer> {
    client: &'a AsyncClient<'a, X>,
    socket: Handle,
    local: core::net::SocketAddrV4,
}

/// UDP socket bound to a port (multiple/unspecified IPs).
///
/// Created via [`UdpStack::bind_multiple()`].
/// Implements [`UnconnectedUdp`] for receiving on multiple interfaces.
///
/// # Lifetime
/// Holds a reference to the parent AsyncClient for the duration of its use.
///
/// # Drop Behavior
/// The socket is automatically closed when dropped.
pub struct AsyncUdpMultiplyBound<'a, X: Xfer> {
    client: &'a AsyncClient<'a, X>,
    socket: Handle,
    local_port: u16,
}

impl<X: Xfer> Drop for AsyncUdpConnected<'_, X> {
    fn drop(&mut self) {
        self.client.close_udp_handle(self.socket);
    }
}

impl<X: Xfer> Drop for AsyncUdpUniquelyBound<'_, X> {
    fn drop(&mut self) {
        self.client.close_udp_handle(self.socket);
    }
}

impl<X: Xfer> Drop for AsyncUdpMultiplyBound<'_, X> {
    fn drop(&mut self) {
        self.client.close_udp_handle(self.socket);
    }
}

impl<X: Xfer> AsyncClient<'_, X> {
    /// Get or create the UDP socket for UnconnectedUdp operations
    fn get_or_create_udp_socket(&self) -> Result<Handle, StackError> {
        let mut socket_opt = self.udp_socket.borrow_mut();

        if let Some(handle) = *socket_opt {
            // Socket already exists, return it
            crate::debug!("Reusing existing UDP socket: {:?}", handle);
            Ok(handle)
        } else {
            // Create new socket
            let session_id = self.get_next_session_id();
            let mut callbacks = self.callbacks.borrow_mut();
            let handle = callbacks
                .udp_sockets
                .add(session_id)
                .ok_or(StackError::OutOfSockets)?;

            crate::debug!("Created new UDP socket: {:?}", handle);

            // Store it for reuse
            *socket_opt = Some(handle);
            Ok(handle)
        }
    }

    /// Generic polling loop for UDP operations.
    /// Polls an operation until it completes, handling dispatch and yielding.
    async fn poll_until_ready<R, F>(&self, mut poll_fn: F) -> Result<R, StackError>
    where
        F: FnMut(
            &mut crate::manager::Manager<X>,
            &mut crate::stack::socket_callbacks::SocketCallbacks,
        ) -> Result<Option<R>, StackError>,
    {
        loop {
            self.dispatch_events()?;

            let result = {
                let mut manager = self.manager.borrow_mut();
                let mut callbacks = self.callbacks.borrow_mut();
                poll_fn(&mut manager, &mut callbacks)?
            };

            if let Some(value) = result {
                return Ok(value);
            }

            self.yield_once().await;
        }
    }

    /// Close the UDP socket if it exists
    fn close_udp_socket(&self) {
        let mut socket_opt = self.udp_socket.borrow_mut();

        if let Some(handle) = socket_opt.take() {
            // Use try_borrow_mut to avoid panicking in Drop if already borrowed
            // If borrows fail, we can't clean up, but that's acceptable - Drop must not panic
            if let (Ok(mut manager), Ok(mut callbacks)) = (
                self.manager.try_borrow_mut(),
                self.callbacks.try_borrow_mut(),
            ) {
                if let Some((sock, _op)) = callbacks.udp_sockets.get(handle) {
                    let _ = manager.send_close(*sock);
                    callbacks.udp_sockets.remove(handle);
                }
            }
        }
    }

    /// Close and remove a UDP socket handle from the callback store.
    /// Used by Drop implementations to ensure consistent cleanup.
    pub(crate) fn close_udp_handle(&self, handle: Handle) {
        // Use try_borrow_mut to avoid panicking in Drop if already borrowed
        if let (Ok(mut manager), Ok(mut callbacks)) = (
            self.manager.try_borrow_mut(),
            self.callbacks.try_borrow_mut(),
        ) {
            if let Some((sock, _)) = callbacks.udp_sockets.get(handle) {
                let _ = manager.send_close(*sock);
                callbacks.udp_sockets.remove(handle);
            }
        }
    }

    /// Bind the UDP socket to a specific local port
    ///
    /// This allows the socket to receive UDP packets sent to the specified port.
    /// The socket will be bound to 0.0.0.0:port (all interfaces).
    ///
    /// Note: The socket must be created before calling bind. This method will
    /// create a socket if one doesn't exist yet.
    ///
    /// # Arguments
    /// * `local_port` - The local port number to bind to (1-65535)
    ///
    /// # Returns
    /// * `Ok(())` - Bind successful
    /// * `Err(StackError)` - Bind failed (port in use, invalid port, etc.)
    pub async fn bind_udp(&mut self, local_port: u16) -> Result<(), StackError> {
        crate::info!("bind_udp: Starting bind to port {}", local_port);

        // Get or create UDP socket
        let handle = self.get_or_create_udp_socket()?;
        crate::info!("bind_udp: Got UDP socket handle {:?}", handle);

        // Use new helper method
        self.bind_socket_to_port(handle, local_port).await?;

        // Store bind address in the per-socket array (reusing udp_socket_connect_addr)
        let bind_addr = self.get_actual_local_ip(local_port)?;
        let mut callbacks = self.callbacks.borrow_mut();
        if let Some((sock, _)) = callbacks.udp_sockets.get(handle) {
            let socket_index = sock.v as usize - crate::stack::socket_callbacks::NUM_TCP_SOCKETS;
            callbacks.udp_socket_connect_addr[socket_index] = Some(bind_addr);
        }

        crate::info!("Successfully bound to port {}", local_port);
        Ok(())
    }
}

impl<'a, X: Xfer> UdpStack for ClientStack<'a, X> {
    type Error = StackError;
    type Connected = AsyncUdpConnected<'a, X>;
    type UniquelyBound = AsyncUdpUniquelyBound<'a, X>;
    type MultiplyBound = AsyncUdpMultiplyBound<'a, X>;

    async fn connect(
        &self,
        remote: core::net::SocketAddr,
    ) -> Result<(core::net::SocketAddr, Self::Connected), Self::Error> {
        self.connect_from(core::net::SocketAddr::from(([0, 0, 0, 0], 0)), remote)
            .await
    }

    async fn connect_from(
        &self,
        local: core::net::SocketAddr,
        remote: core::net::SocketAddr,
    ) -> Result<(core::net::SocketAddr, Self::Connected), Self::Error> {
        // 1. Validate IPv4
        let local_v4 = match local {
            core::net::SocketAddr::V4(a) => a,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };
        let remote_v4 = match remote {
            core::net::SocketAddr::V4(a) => a,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // 2. Validate remote port
        if remote_v4.port() == 0 {
            return Err(StackError::InvalidParameters);
        }

        // 3. Close existing cached UDP socket
        self.0.close_existing_udp_socket()?;

        // 4. Allocate new socket
        let handle = self.0.allocate_udp_socket()?;

        // 5. Bind if local port specified
        if local_v4.port() != 0 {
            self.0.bind_socket_to_port(handle, local_v4.port()).await?;
        }

        // 6. Resolve local address
        let resolved_local = if local_v4.ip().is_unspecified() {
            self.0.get_actual_local_ip(local_v4.port())?
        } else {
            local_v4
        };

        // 7. Return wrapper with plain reference
        Ok((
            core::net::SocketAddr::V4(resolved_local),
            AsyncUdpConnected {
                client: self.0,
                socket: handle,
                remote: remote_v4,
            },
        ))
    }

    async fn bind_single(
        &self,
        local: core::net::SocketAddr,
    ) -> Result<(core::net::SocketAddr, Self::UniquelyBound), Self::Error> {
        let local_v4 = match local {
            core::net::SocketAddr::V4(a) => a,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // Reject port 0 - hardware binding requires explicit port
        if local_v4.port() == 0 {
            return Err(StackError::InvalidParameters);
        }

        self.0.close_existing_udp_socket()?;
        let handle = self.0.allocate_udp_socket()?;
        self.0.bind_socket_to_port(handle, local_v4.port()).await?;

        let resolved_local = if local_v4.ip().is_unspecified() {
            self.0.get_actual_local_ip(local_v4.port())?
        } else {
            local_v4
        };

        Ok((
            core::net::SocketAddr::V4(resolved_local),
            AsyncUdpUniquelyBound {
                client: self.0,
                socket: handle,
                local: resolved_local,
            },
        ))
    }

    async fn bind_multiple(
        &self,
        local: core::net::SocketAddr,
    ) -> Result<Self::MultiplyBound, Self::Error> {
        let local_v4 = match local {
            core::net::SocketAddr::V4(a) => a,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // Reject port 0 - hardware binding requires explicit port
        if local_v4.port() == 0 {
            return Err(StackError::InvalidParameters);
        }

        self.0.close_existing_udp_socket()?;
        let handle = self.0.allocate_udp_socket()?;
        self.0.bind_socket_to_port(handle, local_v4.port()).await?;

        Ok(AsyncUdpMultiplyBound {
            client: self.0,
            socket: handle,
            local_port: local_v4.port(),
        })
    }
}

impl<X: Xfer> ConnectedUdp for AsyncUdpConnected<'_, X> {
    type Error = StackError;

    async fn send(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        // Reuse existing UdpSendOp with stored remote address
        let mut udp_send_op = UdpSendOp::new(self.socket, self.remote, data);

        // Use generic polling helper
        self.client
            .poll_until_ready(|manager, callbacks| udp_send_op.poll_impl(manager, callbacks))
            .await
    }

    async fn receive_into(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        // Reuse existing UdpReceiveOp
        let mut udp_receive_op = UdpReceiveOp::new(self.socket, buffer);

        // Use generic polling helper, extract only len from result
        self.client
            .poll_until_ready(|manager, callbacks| udp_receive_op.poll_impl(manager, callbacks))
            .await
            .map(|(len, _remote_addr)| len)
    }
}

impl<X: Xfer> UnconnectedUdp for AsyncClient<'_, X> {
    type Error = StackError;

    async fn send(
        &mut self,
        local: core::net::SocketAddr,
        remote: core::net::SocketAddr,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        // Note: Can't use {:?} with SocketAddr in defmt, so skip detailed logging here

        // Convert to IPv4 addresses (IPv6 not supported)
        let local_v4 = match local {
            core::net::SocketAddr::V4(addr) => addr,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // WINC1500 hardware does not support binding to specific source ports
        // for unconnected UDP sends. Fail fast if caller requests a specific port.
        if local_v4.port() != 0 {
            return Err(StackError::InvalidParameters);
        }

        let remote_v4 = match remote {
            core::net::SocketAddr::V4(addr) => addr,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // Port 0 is not a valid destination port
        if remote_v4.port() == 0 {
            return Err(StackError::InvalidParameters);
        }

        // Get or create UDP socket (reused across send/receive)
        let handle = self.get_or_create_udp_socket()?;

        // Create UDP send operation
        let mut udp_send_op = UdpSendOp::new(handle, remote_v4, data);

        // Poll until send completes
        self.poll_until_ready(|manager, callbacks| udp_send_op.poll_impl(manager, callbacks))
            .await?;

        Ok(())
    }

    async fn receive_into(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<(usize, core::net::SocketAddr, core::net::SocketAddr), Self::Error> {
        // Get or create UDP socket
        let handle = self.get_or_create_udp_socket()?;

        // Create receive operation
        let mut udp_receive_op = UdpReceiveOp::new(handle, buffer);

        // Poll until data received
        let (len, remote_addr) = self
            .poll_until_ready(|manager, callbacks| udp_receive_op.poll_impl(manager, callbacks))
            .await?;

        // Look up local address from per-socket array
        let local_addr = {
            let mut callbacks = self.callbacks.borrow_mut();
            if let Some((sock, _)) = callbacks.udp_sockets.get(handle) {
                let socket_index =
                    sock.v as usize - crate::stack::socket_callbacks::NUM_TCP_SOCKETS;
                callbacks.udp_socket_connect_addr[socket_index]
                    .map(core::net::SocketAddr::V4)
                    .unwrap_or_else(|| {
                        // Unbound - return 0.0.0.0:0 (ephemeral port)
                        core::net::SocketAddr::V4(core::net::SocketAddrV4::new(
                            core::net::Ipv4Addr::UNSPECIFIED,
                            0,
                        ))
                    })
            } else {
                // Socket not found - return unspecified
                core::net::SocketAddr::V4(core::net::SocketAddrV4::new(
                    core::net::Ipv4Addr::UNSPECIFIED,
                    0,
                ))
            }
        };

        Ok((len, local_addr, remote_addr))
    }
}

impl<X: Xfer> UnconnectedUdp for AsyncUdpUniquelyBound<'_, X> {
    type Error = StackError;

    async fn send(
        &mut self,
        local: core::net::SocketAddr,
        remote: core::net::SocketAddr,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        // Validate IPv4
        let remote_v4 = match remote {
            core::net::SocketAddr::V4(a) => a,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // Validate local matches stored address (all builds)
        if let core::net::SocketAddr::V4(local_v4) = local {
            if local_v4 != self.local && !local_v4.ip().is_unspecified() {
                return Err(StackError::InvalidParameters);
            }
        }

        // Reuse UdpSendOp
        let mut udp_send_op = UdpSendOp::new(self.socket, remote_v4, data);

        // Poll until send completes
        self.client
            .poll_until_ready(|manager, callbacks| udp_send_op.poll_impl(manager, callbacks))
            .await?;

        Ok(())
    }

    async fn receive_into(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<(usize, core::net::SocketAddr, core::net::SocketAddr), Self::Error> {
        // Similar to ConnectedUdp but returns addresses
        let mut udp_receive_op = UdpReceiveOp::new(self.socket, buffer);

        // Poll until data received
        self.client
            .poll_until_ready(|manager, callbacks| udp_receive_op.poll_impl(manager, callbacks))
            .await
            .map(|(len, remote_addr)| (len, core::net::SocketAddr::V4(self.local), remote_addr))
    }
}

impl<X: Xfer> UnconnectedUdp for AsyncUdpMultiplyBound<'_, X> {
    type Error = StackError;

    async fn send(
        &mut self,
        local: core::net::SocketAddr,
        remote: core::net::SocketAddr,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        // Validate IPv4
        let remote_v4 = match remote {
            core::net::SocketAddr::V4(a) => a,
            core::net::SocketAddr::V6(_) => return Err(StackError::InvalidParameters),
        };

        // Validate port matches (only validate port for multiply bound)
        if local.port() != self.local_port && local.port() != 0 {
            return Err(StackError::InvalidParameters);
        }

        // Reuse UdpSendOp
        let mut udp_send_op = UdpSendOp::new(self.socket, remote_v4, data);

        // Poll until send completes
        self.client
            .poll_until_ready(|manager, callbacks| udp_send_op.poll_impl(manager, callbacks))
            .await?;

        Ok(())
    }

    async fn receive_into(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<(usize, core::net::SocketAddr, core::net::SocketAddr), Self::Error> {
        let mut udp_receive_op = UdpReceiveOp::new(self.socket, buffer);

        // Poll until data received
        let (len, remote_addr) = self
            .client
            .poll_until_ready(|manager, callbacks| udp_receive_op.poll_impl(manager, callbacks))
            .await?;

        // Get actual local address for this packet
        // Don't mask errors - propagate InvalidState if IP not configured
        let local_addr = self.client.get_actual_local_ip(self.local_port)?;
        Ok((len, core::net::SocketAddr::V4(local_addr), remote_addr))
    }
}

impl<X: Xfer> Drop for AsyncClient<'_, X> {
    fn drop(&mut self) {
        // Clean up UDP socket when AsyncClient is dropped
        self.close_udp_socket();
    }
}
