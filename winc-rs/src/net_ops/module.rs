use crate::error;
use crate::errors::CommError as Error;
use crate::manager::{
    AccessPoint, AuthType, BootState, Credentials, FirmwareInfo, HostName, MacAddress, Manager,
    ProvisioningInfo, SocketOptions, Ssid, TcpSockOpts, UdpSockOpts, WifiChannel,
};
use crate::net_ops::op::OpImpl;
use crate::stack::{
    sock_holder::SocketStore,
    socket_callbacks::{Handle, SocketCallbacks, WifiModuleState},
    StackError,
};
use crate::transfer::Xfer;

#[cfg(feature = "ssl")]
use crate::manager::{SslSockConfig, SslSockOpts};

#[cfg(feature = "flash-rw")]
use crate::manager::BootMode;

// 1 minute max, if no other delays are added
const AP_CONNECT_TIMEOUT_MILLISECONDS: u32 = 60_000;
// Timeout for Provisioning
#[cfg(not(test))]
const PROVISIONING_TIMEOUT: u32 = 60 * 1000;
#[cfg(test)]
const PROVISIONING_TIMEOUT: u32 = 1000;

/// Synchronous operations type.
enum SyncOpType<'a> {
    GetFirmwareVersion {
        info: Option<FirmwareInfo>,
    },
    StopProvisioningMode,
    EnableAccessPoint {
        ap: &'a AccessPoint<'a>,
    },
    DisableAccessPoint,
    SetSocketOption {
        socket: &'a Handle,
        sock_opts: &'a SocketOptions,
    },
    GetWincMacAddress {
        #[cfg(test)]
        test_hook: bool,
        mac: Option<MacAddress>,
    },
}

/// Container for managing synchronous operations.
///
/// This wrapper restricts how the fields of `SyncOpType` can be
/// mutably accessed when implementing the `OpImpl` trait. By avoiding
/// direct use of the enum, it prevents unintended mutable access to
/// all fields, allowing mutation only where explicitly required.
pub(crate) struct SyncOp<'a> {
    /// The synchronous operation being managed.
    op: SyncOpType<'a>,
}

/// Structure to hold configuration for station mode.
pub(crate) struct StationMode<'a> {
    ssid: Option<&'a Ssid>,
    credentials: Option<&'a Credentials>,
    channel: WifiChannel,
    save_credentials: bool,
    use_defaults: bool,
}

/// Structure to hold configuration for Provisioning Mode.
pub(crate) struct ProvisioningMode<'a> {
    ap: &'a AccessPoint<'a>,
    hostname: &'a HostName,
    http_redirect: bool,
    timeout: u32,
}

/// Constructors and helpers for synchronous operations.
impl<'a> SyncOp<'a> {
    /// Sets UDP socket options on the given socket.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    /// * `socket` - The socket handle to configure.
    /// * `udp_sock_opt` - The UDP socket options to apply.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the socket option was successfully applied.
    /// * `StackError` - If an error occurs while applying the socket option.
    fn handle_udp_socket_options<X: Xfer>(
        &self,
        manager: &mut Manager<X>,
        callbacks: &mut SocketCallbacks,
        socket: &Handle,
        udp_sock_opt: &crate::manager::UdpSockOpts,
    ) -> Result<(), StackError> {
        let (sock, _) = callbacks
            .udp_sockets
            .get(*socket)
            .ok_or(StackError::SocketNotFound)?;

        if let UdpSockOpts::ReceiveTimeout(timeout) = udp_sock_opt {
            // Receive timeout are handled by WINC stack not by module.
            sock.set_recv_timeout(*timeout);
        } else {
            manager.send_setsockopt(*sock, udp_sock_opt)?;
        }

        Ok(())
    }

    /// Sets TCP socket options on the given socket.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    /// * `socket` - The socket handle to configure.
    /// * `tcp_sock_opt` - The TCP socket options to apply.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the socket option was successfully applied.
    /// * `StackError` - If an error occurs while applying the socket option.
    fn handle_tcp_socket_options<#[cfg(feature = "ssl")] X: Xfer>(
        &self,
        #[cfg(feature = "ssl")] manager: &mut Manager<X>,
        callbacks: &mut SocketCallbacks,
        socket: &Handle,
        tcp_sock_opt: &crate::manager::TcpSockOpts,
    ) -> Result<(), StackError> {
        let (sock, _) = callbacks
            .tcp_sockets
            .get(*socket)
            .ok_or(StackError::SocketNotFound)?;

        match tcp_sock_opt {
            #[cfg(feature = "ssl")]
            TcpSockOpts::Ssl(ssl_opts) => {
                match *ssl_opts {
                    SslSockOpts::SetSni(_) => {
                        manager.send_ssl_setsockopt(*sock, ssl_opts)?;
                    }
                    SslSockOpts::Config(cfg, en) => {
                        if cfg == SslSockConfig::EnableSSL && en {
                            if (sock.get_ssl_cfg() & u8::from(cfg)) == cfg.into() {
                                return Ok(());
                            } else {
                                manager.send_ssl_sock_create(*sock)?;
                            }
                        }
                        // Set the SSL flags
                        sock.set_ssl_cfg(cfg.into(), en);
                    }
                }
            }
            TcpSockOpts::ReceiveTimeout(timeout) => {
                // Receive timeout are handled by WINC stack not by module.
                sock.set_recv_timeout(*timeout);
            }
        }

        Ok(())
    }

    /// Sets the specified socket option on the given socket.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    /// * `socket` - A socket handle to configure.
    /// * `option` - The socket option to apply.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the socket option was successfully applied.
    /// * `StackError` - If an error occurs while applying the socket option.
    fn set_socket_options_impl<X: Xfer>(
        &self,
        manager: &mut Manager<X>,
        callbacks: &mut SocketCallbacks,
        socket: &Handle,
        sock_opt: &SocketOptions,
    ) -> Result<(), StackError> {
        match sock_opt {
            SocketOptions::Udp(opts) => {
                self.handle_udp_socket_options(manager, callbacks, socket, opts)
            }

            SocketOptions::Tcp(opts) => self.handle_tcp_socket_options(
                #[cfg(feature = "ssl")]
                manager,
                callbacks,
                socket,
                opts,
            ),
        }
    }

    /// Creates a synchronous request to set socket options.
    ///
    /// # Arguments
    ///
    /// * `socket` - A socket handle to configure.
    /// * `option` - The socket option to apply.
    #[inline]
    pub(crate) fn set_socket_options(socket: &'a Handle, sock_opts: &'a SocketOptions) -> Self {
        Self {
            op: SyncOpType::SetSocketOption { socket, sock_opts },
        }
    }

    /// Creates a synchronous request to get WINC firmware version.
    #[inline]
    pub(crate) fn get_firmware_version() -> Self {
        Self {
            op: SyncOpType::GetFirmwareVersion { info: None },
        }
    }

    /// Creates a synchronous request to stop the provisioning mode.
    #[inline]
    pub(crate) fn stop_provisioning_mode() -> Self {
        Self {
            op: SyncOpType::StopProvisioningMode,
        }
    }

    /// Creates a synchronous request to enable the access point.
    #[inline]
    pub(crate) fn enable_access_point(ap: &'a AccessPoint) -> Self {
        Self {
            op: SyncOpType::EnableAccessPoint { ap },
        }
    }

    /// Creates a synchronous request to disable the access point.
    #[inline]
    pub(crate) fn disable_access_point() -> Self {
        Self {
            op: SyncOpType::DisableAccessPoint,
        }
    }

    /// Creates a synchronous request to request mac address of WINC.
    #[inline]
    pub(crate) fn get_winc_mac_address(#[cfg(test)] test_hook: bool) -> Self {
        Self {
            op: SyncOpType::GetWincMacAddress {
                #[cfg(test)]
                test_hook,
                mac: None,
            },
        }
    }

    /// Returns the firmware version if it has been acquired.
    ///
    /// # Returns
    ///
    /// * `Ok(FirmwareInfo)` - WINC firmware version if available.
    /// * `Err(StackError)` - If the firmware version request has not been made or if it failed.
    pub(crate) fn retrieve_firmware_version(&mut self) -> Result<FirmwareInfo, StackError> {
        if let SyncOpType::GetFirmwareVersion { ref mut info } = self.op {
            if let Some(info) = info.take() {
                return Ok(info);
            }
        }

        Err(StackError::InvalidState)
    }

    /// Returns the WINC MAC address if it has been acquired.
    ///
    /// # Returns
    ///
    /// * `Ok(MacAddress)` - WINC MAC address if available.
    /// * `Err(StackError)` - If the MAC address request has not been made or if it failed.
    pub(crate) fn retrieve_winc_mac_address(&mut self) -> Result<MacAddress, StackError> {
        if let SyncOpType::GetWincMacAddress { ref mut mac, .. } = self.op {
            if let Some(mac) = mac.take() {
                return Ok(mac);
            }
        }

        Err(StackError::InvalidState)
    }
}

/// Creates configuration instances for Wi-Fi station mode.
impl<'a> StationMode<'a> {
    /// Creates a new Wi-Fi configuration from the provided credentials.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID of the access point to connect to.
    /// * `credentials` - Authentication credentials for the access point.
    /// * `channel` - The Wi-Fi channel to use when connecting.
    /// * `save_credentials` - Whether the credentials should be stored
    ///   persistently after a successful connection.
    ///
    /// # Returns
    ///
    /// A `StationMode` instance configured with the provided SSID, credentials,
    /// channel, and credential persistence preference.
    pub(crate) fn from_credentials(
        ssid: &'a Ssid,
        credentials: &'a Credentials,
        channel: WifiChannel,
        save_credentials: bool,
    ) -> Self {
        Self {
            ssid: Some(ssid),
            credentials: Some(credentials),
            channel,
            save_credentials,
            use_defaults: false,
        }
    }

    /// Creates a new `StationMode` instance configured to use saved credentials.
    pub(crate) fn from_defaults() -> Self {
        Self {
            ssid: None,
            credentials: None,
            channel: WifiChannel::Channel1,
            save_credentials: false,
            use_defaults: true,
        }
    }
}

/// Creates configuration instances for Wi-Fi provisioning mode.
impl<'a> ProvisioningMode<'a> {
    /// Creates a new `ProvisioningMode` instance with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `ap` - An `AccessPoint` struct containing the SSID, password, and other network details.
    /// * `hostname` - Device domain name. Must not include `.local`.
    /// * `http_redirect` - Whether HTTP redirection is enabled.
    /// * `timeout` - The timeout duration for provisioning, in minutes.
    pub fn new(
        ap: &'a AccessPoint<'a>,
        hostname: &'a HostName,
        http_redirect: bool,
        timeout: u32,
    ) -> Self {
        Self {
            ap,
            hostname,
            http_redirect,
            timeout,
        }
    }
}

/// Handles Wi-Fi connection operations in station mode.
impl<X: Xfer> OpImpl<X> for StationMode<'_> {
    type Output = ();
    type Error = StackError;

    /// Polls the internal state machine and attempts to progress the connection.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(output))` - Operation completed successfully.
    /// * `Ok(None)` - Operation is still in progress.
    /// * `Err(Self::Error)` - An error occurred while polling.
    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut crate::stack::socket_callbacks::SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let state = callbacks.state.clone();

        match state {
            WifiModuleState::Unconnected | WifiModuleState::Provisioning => {
                callbacks.state = WifiModuleState::ConnectingToAp;
                manager.set_operation_timeout(AP_CONNECT_TIMEOUT_MILLISECONDS);
                if self.use_defaults {
                    manager.send_default_connect()?;
                } else {
                    manager.send_connect(
                        self.ssid.ok_or(StackError::InvalidParameters)?,
                        self.credentials.ok_or(StackError::InvalidParameters)?,
                        self.channel,
                        !self.save_credentials,
                    )?;
                }
            }
            WifiModuleState::ConnectionFailed => {
                callbacks.state = WifiModuleState::Unconnected;
                // conn_error should always be Some in ConnectionFailed state,
                // but use defensive fallback just in case
                let res = callbacks
                    .connection_state
                    .conn_error
                    .take()
                    .unwrap_or(crate::manager::WifiConnError::Unhandled);
                return Err(StackError::ApJoinFailed(res));
            }
            WifiModuleState::ConnectingToAp => {
                let mut timeout = manager.get_operation_timeout();
                if timeout == 0 {
                    return Err(StackError::GeneralTimeout);
                }
                timeout -= 1;
                manager.set_operation_timeout(timeout);
            }
            WifiModuleState::ConnectedToAp => {
                return Ok(Some(()));
            }
            _ => {
                return Err(StackError::InvalidState);
            }
        }

        Ok(None)
    }
}

/// Manages the operation to boot the WINC module.
impl<X: Xfer> OpImpl<X> for BootState {
    type Output = ();
    type Error = StackError;

    /// Polls the internal state machine and attempts to boot the chip.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(output))` - Operation completed successfully.
    /// * `Ok(None)` - Operation is still in progress.
    /// * `Err(Self::Error)` - An error occurred while polling.
    fn poll_impl(
        &mut self,
        manager: &mut Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        let state = callbacks.state.clone();
        match state {
            WifiModuleState::Reset => {
                manager.set_crc_state(true);
                callbacks.state = WifiModuleState::Starting;
            }
            WifiModuleState::Starting => {
                let result = manager.boot_the_chip(self)?;
                if result {
                    #[cfg(feature = "flash-rw")]
                    {
                        if self.get_boot_mode() == BootMode::Download {
                            callbacks.state = WifiModuleState::DownloadMode;
                            crate::info!("Chip booted into download mode.");
                            return Ok(Some(()));
                        }
                    }
                    callbacks.state = WifiModuleState::Unconnected;
                    return Ok(Some(()));
                }
            }
            #[cfg(feature = "flash-rw")]
            WifiModuleState::DownloadMode => {
                if self.get_boot_mode() == BootMode::Download {
                    crate::info!("Chip is already in download mode.");
                    return Ok(Some(()));
                }
                return Err(StackError::InvalidState);
            }
            _ => return Err(StackError::InvalidState),
        }

        Ok(None)
    }
}

/// Manages the synchronous operation.
impl<'a, X: Xfer> OpImpl<X> for SyncOp<'a> {
    type Output = ();
    type Error = StackError;

    /// Polls the state machine once and manages to requested
    /// synchronous operation.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(output))` - The operation completed successfully.
    /// * `Ok(None)` - The operation is still in progress.
    /// * `Err(Self::Error)` - An error occurred while polling.
    fn poll_impl(
        &mut self,
        manager: &mut Manager<X>,
        callbacks: &mut SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        match self.op {
            SyncOpType::GetFirmwareVersion { ref mut info } => {
                if info.is_some() {
                    return Err(StackError::InvalidState);
                }

                info.replace(manager.get_firmware_ver_full()?);
            }

            SyncOpType::StopProvisioningMode => {
                if callbacks.state == WifiModuleState::Provisioning {
                    manager.send_stop_provisioning()?;
                } else {
                    return Err(StackError::InvalidState);
                }

                // change the state to unconnected
                callbacks.state = WifiModuleState::Unconnected;
            }
            SyncOpType::EnableAccessPoint { ap } => {
                if callbacks.state == WifiModuleState::Unconnected {
                    let auth: AuthType = ap.key.into();
                    if auth == AuthType::S802_1X {
                        crate::error!("Enterprise Security is not supported in access point mode");
                        return Err(StackError::InvalidParameters);
                    }
                    manager.send_enable_access_point(ap)?;
                    callbacks.state = WifiModuleState::AccessPoint;
                } else {
                    return Err(StackError::InvalidState);
                }
            }
            SyncOpType::DisableAccessPoint => {
                if callbacks.state == WifiModuleState::AccessPoint {
                    manager.send_disable_access_point()?;
                    callbacks.state = WifiModuleState::Unconnected;
                } else {
                    return Err(StackError::InvalidState);
                }
            }
            SyncOpType::SetSocketOption { socket, sock_opts } => {
                self.set_socket_options_impl(manager, callbacks, socket, sock_opts)?;
            }
            SyncOpType::GetWincMacAddress {
                #[cfg(test)]
                test_hook,
                ref mut mac,
            } => {
                if mac.is_some() {
                    return Err(StackError::InvalidState);
                }

                mac.replace(manager.read_otp_mac_address(
                    #[cfg(test)]
                    test_hook,
                )?);
            }
        }

        Ok(Some(()))
    }
}

/// `OpImpl` trait implementation for `ProvisioningMode`.
impl<'a, X: Xfer> OpImpl<X> for ProvisioningMode<'a> {
    type Output = ProvisioningInfo;
    type Error = StackError;

    /// Polls the state machine and attempts to initiate provisioning mode.
    ///
    /// # Arguments
    ///
    /// * `manager` - The stack manager handling low-level operations.
    /// * `callbacks` - Socket callback handlers.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(output))` - Operation completed successfully.
    /// * `Ok(None)` - Operation is still in progress.
    /// * `Err(Self::Error)` - An error occurred while polling.
    fn poll_impl(
        &mut self,
        manager: &mut crate::manager::Manager<X>,
        callbacks: &mut crate::stack::socket_callbacks::SocketCallbacks,
    ) -> Result<Option<Self::Output>, Self::Error> {
        match &mut callbacks.state {
            WifiModuleState::Unconnected | WifiModuleState::ConnectedToAp => {
                let auth = <Credentials as Into<AuthType>>::into(self.ap.key);

                if auth == AuthType::S802_1X {
                    error!("Enterprise Security in provisioning mode is not supported");
                    return Err(StackError::InvalidParameters);
                }

                manager.send_start_provisioning(self.ap, self.hostname, self.http_redirect)?;

                callbacks.state = WifiModuleState::Provisioning;
                callbacks.provisioning_info = None;
            }
            WifiModuleState::Provisioning => match &mut callbacks.provisioning_info {
                None => {
                    manager
                        .set_operation_timeout(self.timeout.saturating_mul(PROVISIONING_TIMEOUT));
                    callbacks.provisioning_info = Some(None);
                }
                Some(result) => {
                    if let Some(info) = result.take() {
                        if info.status {
                            return Ok(Some(info));
                        }
                        callbacks.provisioning_info = None;
                        return Err(StackError::WincWifiFail(Error::Failed));
                    } else {
                        let mut timeout = manager.get_operation_timeout();
                        if timeout == 0 {
                            return Err(StackError::GeneralTimeout);
                        }
                        timeout -= 1;
                        manager.set_operation_timeout(timeout);
                    }
                }
            },
            _ => {
                return Err(StackError::InvalidState);
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_async_retrieve_winc_fw_ver() {
        let mut op = SyncOp::get_firmware_version();
        let result = op.retrieve_firmware_version();

        assert!(result.is_err());
        assert_eq!(result.err(), Some(StackError::InvalidState));
    }

    #[test]
    fn test_async_retrieve_winc_mac_addr() {
        let mut op = SyncOp::get_winc_mac_address(false);
        let result = op.retrieve_winc_mac_address();

        assert!(result.is_err());
        assert_eq!(result.err(), Some(StackError::InvalidState));
    }
}
