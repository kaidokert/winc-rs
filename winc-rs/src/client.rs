use crate::manager::Manager;
use crate::transfer::Xfer;

mod dns;
#[cfg(feature = "ethernet")]
mod ethernet;
#[cfg(feature = "flash-rw")]
mod flash;
#[cfg(feature = "experimental-ota")]
mod ota;
mod prng;
#[cfg(feature = "ssl")]
mod ssl;
mod tcp_stack;
mod udp_stack;
mod wifi_module;

pub use crate::stack::StackError;

pub use crate::stack::socket_callbacks::ClientSocketOp;
use crate::stack::socket_callbacks::SocketCallbacks;
pub use crate::stack::socket_callbacks::{Handle, PingResult};

/// Driver-level network counters (feature `net-stats`).
///
/// The WINC runs the TCP/IP stack on-chip, so the host never sees L2 frames — these
/// count what crosses the *driver API*: successful socket send/receive calls and their
/// payload byte totals, split by transport. It is therefore app-layer TCP/UDP traffic
/// (ARP/DHCP/DNS/ICMP happen inside the chip and are invisible), and `*_ops` are socket
/// calls, **not** wire packets (the WINC fragments/reassembles internally). `dns_queries`
/// counts host-name lookups issued. All counters wrap at `u32`.
#[cfg(feature = "net-stats")]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct NetStats {
    pub tcp_tx_bytes: u32,
    pub tcp_rx_bytes: u32,
    pub tcp_tx_ops: u32,
    pub tcp_rx_ops: u32,
    pub udp_tx_bytes: u32,
    pub udp_rx_bytes: u32,
    pub udp_tx_ops: u32,
    pub udp_rx_ops: u32,
    pub dns_queries: u32,
}

/// Process-global network counters (feature `net-stats`). Global rather than per-client
/// so they can be read without borrowing the [`WincClient`] — the normal case is one WINC
/// per system. Incremented from the socket send/receive paths.
#[cfg(feature = "net-stats")]
pub(crate) mod counters {
    use super::NetStats;
    use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

    static TCP_TX_BYTES: AtomicU32 = AtomicU32::new(0);
    static TCP_RX_BYTES: AtomicU32 = AtomicU32::new(0);
    static TCP_TX_OPS: AtomicU32 = AtomicU32::new(0);
    static TCP_RX_OPS: AtomicU32 = AtomicU32::new(0);
    static UDP_TX_BYTES: AtomicU32 = AtomicU32::new(0);
    static UDP_RX_BYTES: AtomicU32 = AtomicU32::new(0);
    static UDP_TX_OPS: AtomicU32 = AtomicU32::new(0);
    static UDP_RX_OPS: AtomicU32 = AtomicU32::new(0);
    static DNS_QUERIES: AtomicU32 = AtomicU32::new(0);

    pub(crate) fn tcp_tx(bytes: usize) {
        TCP_TX_BYTES.fetch_add(bytes as u32, Relaxed);
        TCP_TX_OPS.fetch_add(1, Relaxed);
    }
    pub(crate) fn tcp_rx(bytes: usize) {
        TCP_RX_BYTES.fetch_add(bytes as u32, Relaxed);
        TCP_RX_OPS.fetch_add(1, Relaxed);
    }
    pub(crate) fn udp_tx(bytes: usize) {
        UDP_TX_BYTES.fetch_add(bytes as u32, Relaxed);
        UDP_TX_OPS.fetch_add(1, Relaxed);
    }
    pub(crate) fn udp_rx(bytes: usize) {
        UDP_RX_BYTES.fetch_add(bytes as u32, Relaxed);
        UDP_RX_OPS.fetch_add(1, Relaxed);
    }
    pub(crate) fn dns_query() {
        DNS_QUERIES.fetch_add(1, Relaxed);
    }

    pub(crate) fn snapshot() -> NetStats {
        NetStats {
            tcp_tx_bytes: TCP_TX_BYTES.load(Relaxed),
            tcp_rx_bytes: TCP_RX_BYTES.load(Relaxed),
            tcp_tx_ops: TCP_TX_OPS.load(Relaxed),
            tcp_rx_ops: TCP_RX_OPS.load(Relaxed),
            udp_tx_bytes: UDP_TX_BYTES.load(Relaxed),
            udp_rx_bytes: UDP_RX_BYTES.load(Relaxed),
            udp_tx_ops: UDP_TX_OPS.load(Relaxed),
            udp_rx_ops: UDP_RX_OPS.load(Relaxed),
            dns_queries: DNS_QUERIES.load(Relaxed),
        }
    }
    pub(crate) fn reset() {
        for a in [
            &TCP_TX_BYTES,
            &TCP_RX_BYTES,
            &TCP_TX_OPS,
            &TCP_RX_OPS,
            &UDP_TX_BYTES,
            &UDP_RX_BYTES,
            &UDP_TX_OPS,
            &UDP_RX_OPS,
            &DNS_QUERIES,
        ] {
            a.store(0, Relaxed);
        }
    }
}

/// Read a snapshot of the driver-level network counters (feature `net-stats`).
///
/// Free-function form of [`WincClient::net_stats`], readable even while the client is
/// borrowed elsewhere (e.g. by a TLS stream). See [`NetStats`].
#[cfg(feature = "net-stats")]
pub fn net_stats() -> NetStats {
    counters::snapshot()
}

/// Reset all driver-level network counters to zero (feature `net-stats`).
#[cfg(feature = "net-stats")]
pub fn reset_net_stats() {
    counters::reset()
}

/// Client for the WincWifi chip.
///
/// This manages the state of the chip and
/// network connections
pub struct WincClient<'a, X: Xfer> {
    manager: Manager<X>,
    poll_loop_delay_us: u32,
    callbacks: SocketCallbacks,
    next_session_id: u16,
    boot: Option<crate::manager::BootState>,
    operation_countdown: u32,
    dns_op: Option<crate::ops::net_ops::dns::DnsOp>,
    phantom: core::marker::PhantomData<&'a ()>,
    #[cfg(test)]
    debug_callback: Option<&'a mut dyn FnMut(&mut SocketCallbacks)>,
}

impl<X: Xfer> WincClient<'_, X> {
    const TCP_SOCKET_BACKLOG: u8 = 4;
    const LISTEN_TIMEOUT: u32 = 100;
    const BIND_TIMEOUT: u32 = 100;
    const DNS_TIMEOUT: u32 = 1000;
    const POLL_LOOP_DELAY_US: u32 = 100;
    /// Create a new WincClient..
    ///
    /// # Arguments
    ///
    /// * `transfer` - The transfer implementation to use for client,
    ///   typically a struct wrapping SPI communication.
    ///
    ///  See [Xfer] for details how to implement a transfer struct.
    pub fn new(transfer: X) -> Self {
        let manager = Manager::from_xfer(transfer);
        Self {
            manager,
            callbacks: SocketCallbacks::new(),
            poll_loop_delay_us: Self::POLL_LOOP_DELAY_US,
            next_session_id: 0,
            boot: None,
            operation_countdown: 0,
            dns_op: None,
            phantom: core::marker::PhantomData,
            #[cfg(test)]
            debug_callback: None,
        }
    }

    /// Snapshot of the driver-level network counters (feature `net-stats`).
    ///
    /// Equivalent to the free [`net_stats()`](crate::net_stats) function — the counters
    /// are process-global (one WINC per system), so they can also be read without a
    /// borrow of the client (useful when the client is borrowed elsewhere, e.g. by a
    /// TLS stream). See [`NetStats`] for exactly what is (and isn't) counted.
    #[cfg(feature = "net-stats")]
    pub fn net_stats(&self) -> NetStats {
        net_stats()
    }

    /// Reset all network counters to zero (feature `net-stats`).
    #[cfg(feature = "net-stats")]
    pub fn reset_net_stats(&mut self) {
        reset_net_stats()
    }
    // Todo: remove this
    fn delay_us(&mut self, delay: u32) {
        self.manager.delay_us(delay)
    }
    fn get_next_session_id(&mut self) -> u16 {
        let ret = self.next_session_id;
        self.next_session_id += 1;
        ret
    }

    fn test_hook(&mut self) {
        #[cfg(test)]
        if let Some(callback) = &mut self.debug_callback {
            callback(&mut self.callbacks);
        }
    }

    /// Poll the chip for new events.
    ///
    /// # Returns
    ///
    /// * `()` - No error occurred while polling the chip for new events.
    /// * `StackError` - An error occurred while polling the chip for new events.
    fn dispatch_events(&mut self) -> Result<(), StackError> {
        self.test_hook();
        self.manager
            .dispatch_events_new(&mut self.callbacks)
            .map_err(StackError::DispatchError)
    }

    /// Poll the chip for new events. If "irq" is enabled, it will wait for an interrupt on the IRQ
    /// pin of the WiFi chip before polling for new events. If "irq" is not enabled,
    /// it will poll the chip for new events without waiting.
    ///
    /// # Returns
    ///
    /// * `()` - No error occurred while polling for new events.
    /// * `StackError` - An error occurred while polling for new events.
    fn dispatch_events_may_wait(&mut self) -> Result<(), StackError> {
        self.test_hook();
        self.manager
            .dispatch_events_may_wait(&mut self.callbacks)
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

            self.delay_us(self.poll_loop_delay_us);
            self.dispatch_events()?;
            timeout -= self.poll_loop_delay_us as i32;
            elapsed += self.poll_loop_delay_us;
        }
    }

    /// Polls the provided operation once.
    ///
    /// # Arguments
    ///
    /// * `op` - The operation to be polled.
    ///
    /// # Returns
    ///
    /// * `Ok(O::Output)` - Operation completed successfully.
    /// * `Err(StackError::ContinueOperation)` - Operation is still in progress.
    /// * `Err(StackError)` - Operation failed.
    fn poll_once<O: crate::ops::op::OpImpl<X, Error = StackError>>(
        &mut self,
        op: &mut O,
    ) -> Result<O::Output, StackError> {
        let result = op.poll_impl(&mut self.manager, &mut self.callbacks);

        match result {
            Ok(Some(result)) => Ok(result),
            Ok(None) => Err(StackError::ContinueOperation),
            Err(e) => Err(e),
        }
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
        fn recv(&mut self, _: &mut [u8]) -> Result<(), crate::errors::CommError> {
            Ok(())
        }
        fn send(&mut self, _: &[u8]) -> Result<(), crate::errors::CommError> {
            Ok(())
        }
    }

    pub(crate) fn make_test_client<'a>() -> WincClient<'a, MockTransfer> {
        let mut client = WincClient::new(MockTransfer::default());
        client.manager.set_unit_test_mode();
        client
    }
}

#[cfg(test)]
mod tests {
    // Helper function to compute CRC16 for test data integrity verification
    pub(crate) fn compute_crc16(input: &[u8]) -> u16 {
        use crc_any::CRC;
        let mut crc = CRC::crc16aug_ccitt();
        crc.digest(&[0x99, 0xc0]); // reset crc to 0xFFFF
        crc.digest(input);
        crc.get_crc() as u16
    }

    // Generate a predictable, sequential pattern of u32 values for testing
    pub(crate) fn generate_test_pattern(buffer: &mut [u8]) {
        assert!(buffer.len() % 4 == 0, "Buffer size must be a multiple of 4");
        let mut val: u32 = 0;
        for chunk in buffer.chunks_mut(4) {
            chunk.copy_from_slice(&val.to_be_bytes());
            val = val.wrapping_add(1);
        }
    }

    #[test]
    fn test_winc_client() {}

    #[test]
    fn test_poll_once_cont_op() {
        #[derive(Default)]
        struct Test;

        impl<X: super::Xfer> crate::ops::op::OpImpl<X> for Test {
            type Error = crate::StackError;
            type Output = ();

            fn poll_impl(
                &mut self,
                _manager: &mut crate::manager::Manager<X>,
                _callbacks: &mut crate::stack::socket_callbacks::SocketCallbacks,
            ) -> Result<Option<Self::Output>, Self::Error> {
                Ok(None)
            }
        }

        let mut test = Test::default();
        let mut client = super::test_shared::make_test_client();
        let result = client.poll_once(&mut test);

        assert_eq!(result, Err(crate::StackError::ContinueOperation));
    }
}
