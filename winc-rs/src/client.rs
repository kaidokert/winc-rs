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
    /// Bytes handed to `on_recv` by the event listener, i.e. received from the
    /// WINC. `tcp_rx_bytes` counts bytes *returned to the caller*, so the two
    /// differ by exactly what the driver lost internally.
    pub chip_rx_bytes: u32,
    /// `on_recv` deliveries discarded because the socket was not awaiting a
    /// receive. That path only logs; the bytes are gone and no return value
    /// reports it.
    pub rx_dropped_ops: u32,
    pub rx_dropped_bytes: u32,
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
    /// Driver-level network counters (feature `net-stats`).
    ///
    /// A field, not a set of `static`s. They were process-global "so they can be
    /// read without borrowing the WincClient" -- convenient, but it puts them in
    /// the image's shared `.bss`, and a caller running unprivileged behind an MPU
    /// has no grant for that. On Astrum the first `send()` took a DACCVIOL at
    /// `TCP_TX_BYTES`. Every increment site is already inside a `&mut self`
    /// method, so ownership costs nothing.
    #[cfg(feature = "net-stats")]
    stats: NetStats,
    operation_countdown: u32,
    dns_op: Option<crate::ops::net_ops::dns::DnsOp>,
    phantom: core::marker::PhantomData<&'a ()>,
    #[cfg(test)]
    debug_callback: Option<&'a mut dyn FnMut(&mut SocketCallbacks)>,
}

impl<X: Xfer> WincClient<'_, X> {
    // Counter helpers come in cfg'd pairs so the call sites stay unconditional.
    // An `#[cfg]` directly on the increment expression is an attribute on an
    // expression, which is still unstable.
    #[cfg(feature = "net-stats")]
    #[inline]
    fn count_tcp_tx(&mut self, n: usize) {
        self.stats.tcp_tx_bytes = self.stats.tcp_tx_bytes.wrapping_add(n as u32);
        self.stats.tcp_tx_ops = self.stats.tcp_tx_ops.wrapping_add(1);
    }
    #[cfg(not(feature = "net-stats"))]
    #[inline]
    fn count_tcp_tx(&mut self, _n: usize) {}

    #[cfg(feature = "net-stats")]
    #[inline]
    fn count_tcp_rx(&mut self, n: usize) {
        self.stats.tcp_rx_bytes = self.stats.tcp_rx_bytes.wrapping_add(n as u32);
        self.stats.tcp_rx_ops = self.stats.tcp_rx_ops.wrapping_add(1);
    }
    #[cfg(not(feature = "net-stats"))]
    #[inline]
    fn count_tcp_rx(&mut self, _n: usize) {}

    #[cfg(feature = "net-stats")]
    #[inline]
    fn count_udp_tx(&mut self, n: usize) {
        self.stats.udp_tx_bytes = self.stats.udp_tx_bytes.wrapping_add(n as u32);
        self.stats.udp_tx_ops = self.stats.udp_tx_ops.wrapping_add(1);
    }
    #[cfg(not(feature = "net-stats"))]
    #[inline]
    fn count_udp_tx(&mut self, _n: usize) {}

    #[cfg(feature = "net-stats")]
    #[inline]
    fn count_udp_rx(&mut self, n: usize) {
        self.stats.udp_rx_bytes = self.stats.udp_rx_bytes.wrapping_add(n as u32);
        self.stats.udp_rx_ops = self.stats.udp_rx_ops.wrapping_add(1);
    }
    #[cfg(not(feature = "net-stats"))]
    #[inline]
    fn count_udp_rx(&mut self, _n: usize) {}

    #[cfg(feature = "net-stats")]
    #[inline]
    fn count_dns_query(&mut self) {
        self.stats.dns_queries = self.stats.dns_queries.wrapping_add(1);
    }
    #[cfg(not(feature = "net-stats"))]
    #[inline]
    fn count_dns_query(&mut self) {}

    /// Snapshot of the driver-level network counters (feature `net-stats`).
    #[cfg(feature = "net-stats")]
    pub fn net_stats(&self) -> NetStats {
        let mut s = self.stats;
        // These live on SocketCallbacks, which is where the event listener
        // delivers into; fold them in at read time.
        s.chip_rx_bytes = self.callbacks.chip_rx_bytes;
        s.rx_dropped_ops = self.callbacks.rx_dropped_ops;
        s.rx_dropped_bytes = self.callbacks.rx_dropped_bytes;
        s
    }

    /// Reset the driver-level network counters (feature `net-stats`).
    #[cfg(feature = "net-stats")]
    pub fn reset_net_stats(&mut self) {
        self.stats = NetStats::default();
    }

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
            #[cfg(feature = "net-stats")]
            stats: NetStats::default(),
            operation_countdown: 0,
            dns_op: None,
            phantom: core::marker::PhantomData,
            #[cfg(test)]
            debug_callback: None,
        }
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
