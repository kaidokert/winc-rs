use crate::{error, info};
use byteorder::{ByteOrder, NetworkEndian};
use core::net::Ipv4Addr;
use cortex_m_systick_countdown::{CountsMillis, MillisCountDown};
use feather::init::get_uptime;
use smoltcp::{
    iface::{Config, Interface, SocketSet, SocketStorage},
    phy::Device,
    socket::{dhcpv4, icmp},
    time::{Duration, Instant},
    wire::{EthernetAddress, Icmpv4Packet, Icmpv4Repr, IpAddress, IpCidr},
};

// Default timeout for polling the interface
const POLL_INTERFACE_TIMEOUT_MSEC: u32 = 10000;
// Max storage size for ICMP storage.
const MAX_ICMP_STORAGE_SIZE: usize = 256;
// Max entries for ICMP metadata.
const MAX_ICMP_META_DATA_ENTRIES: usize = 1;

// Macro to send PING request.
macro_rules! send_icmp_ping {
    ( $repr_type:ident, $packet_type:ident, $ident:expr, $seq_no:expr,
      $echo_payload:expr, $socket:expr, $remote_addr:expr ) => {{
        let icmp_repr = $repr_type::EchoRequest {
            ident: $ident,
            seq_no: $seq_no as u16,
            data: &$echo_payload,
        };

        let icmp_payload = $socket
            .send(icmp_repr.buffer_len(), $remote_addr)
            .map_err(|_| TcpStackError::ReadWriteError)?;

        let icmp_packet = $packet_type::new_unchecked(icmp_payload);
        (icmp_repr, icmp_packet)
    }};
}

// Error types for external TCP stack.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq)]
pub enum TcpStackError {
    ClockError,
    ReadWriteError,
    SocketError,
    IpAddressError,
    IcmpPacketError,
    Timeout,
    InvalidResponse,
}

// Clock for timekeeping.
pub struct Clock<'a, CM: CountsMillis> {
    counter: &'a mut MillisCountDown<'a, CM>,
}

// Container to store ICMP packet data.
pub struct IcmpStorage {
    rx_meta: [icmp::PacketMetadata; MAX_ICMP_META_DATA_ENTRIES],
    tx_meta: [icmp::PacketMetadata; MAX_ICMP_META_DATA_ENTRIES],
    rx_buf: [u8; MAX_ICMP_STORAGE_SIZE],
    tx_buf: [u8; MAX_ICMP_STORAGE_SIZE],
}

// Network Stack Configuration
pub struct Stack<'a, D: Device, CM: CountsMillis> {
    device: &'a mut D,
    clock: Clock<'a, CM>,
    iface: Interface,
    sockets: SocketSet<'a>,
    icmp_storage: IcmpStorage,
}

impl<'a, CM: CountsMillis> Clock<'a, CM> {
    pub fn new(counter: &'a mut MillisCountDown<'a, CM>) -> Clock<'a, CM> {
        Self { counter }
    }

    pub fn now(&self) -> Result<Instant, TcpStackError> {
        let count = get_uptime();
        if let Some(count) = count {
            Ok(Instant::from_millis(count as i64))
        } else {
            error!("Clock not working.");
            Err(TcpStackError::ClockError)
        }
    }

    pub fn delay_millis(&mut self, delay: u32) -> Result<(), TcpStackError> {
        self.counter.start_ms(delay);
        nb::block!(self.counter.wait_ms()).map_err(|_| TcpStackError::ClockError)
    }

    pub fn delay_duration(&mut self, duration: Duration) -> Result<(), TcpStackError> {
        let delay = duration.total_millis() as u32;
        self.delay_millis(delay)
    }
}

impl IcmpStorage {
    pub fn new() -> Self {
        IcmpStorage {
            rx_meta: [icmp::PacketMetadata::EMPTY],
            tx_meta: [icmp::PacketMetadata::EMPTY],
            rx_buf: [0; MAX_ICMP_STORAGE_SIZE],
            tx_buf: [0; MAX_ICMP_STORAGE_SIZE],
        }
    }
}

impl<'a, D: Device, CM: CountsMillis> Stack<'a, D, CM> {
    /// Initializes the storage and network interface of the `smoltcp` stack.
    pub fn new<const SOCK: usize>(
        device: &'a mut D,
        random_seed: u64,
        sock_storage: &'a mut [SocketStorage<'a>; SOCK],
        mac: [u8; 6],
        counter: &'a mut MillisCountDown<'a, CM>,
    ) -> Result<Self, TcpStackError> {
        let mut config =
            Config::new(EthernetAddress([mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]]).into());
        config.random_seed = random_seed;

        let clock = Clock::new(counter);
        let iface = Interface::new(config, device, clock.now()?);

        Ok(Self {
            device: device,
            sockets: SocketSet::new(&mut sock_storage[..]),
            iface: iface,
            clock: clock,
            icmp_storage: IcmpStorage::new(),
        })
    }

    /// Configures the IPv4 settings using DHCP.
    pub fn config_v4(&mut self) -> Result<(), TcpStackError> {
        const DHCP_TIMEOUT_MSEC: i64 = 60_000;

        let mut dhcp_socket = dhcpv4::Socket::new();
        dhcp_socket.set_max_lease_duration(Some(Duration::from_secs(10)));

        let dhcp_handle = self.sockets.add(dhcp_socket);
        let mut first_attempt = true;
        let max_timeout_msec = self.clock.now()?.total_millis() + DHCP_TIMEOUT_MSEC;

        info!("Waiting for DHCP to assign an IP address.");
        loop {
            let timestamp = self.clock.now()?;
            self.iface.poll(timestamp, self.device, &mut self.sockets);
            let event = {
                let dhcp_socket = self.sockets.get_mut::<dhcpv4::Socket>(dhcp_handle);
                dhcp_socket.poll()
            };
            match event {
                None => {}
                Some(dhcpv4::Event::Configured(config)) => {
                    info!("DHCP config acquired!");
                    let ip_octects = config.address.address().octets();
                    info!(
                        "IP address: {}.{}.{}.{}",
                        ip_octects[0], ip_octects[1], ip_octects[2], ip_octects[3]
                    );

                    self.iface.update_ip_addrs(|addrs| {
                        addrs.clear();
                        if addrs.push(IpCidr::Ipv4(config.address)).is_err() {
                            error!("Failed to set IP address");
                        }
                    });

                    if let Some(router) = config.router {
                        let router_octets = router.octets();
                        info!(
                            "Default gateway: {}.{}.{}.{}",
                            router_octets[0], router_octets[1], router_octets[2], router_octets[3]
                        );
                        self.iface
                            .routes_mut()
                            .add_default_ipv4_route(router)
                            .map_err(|_| TcpStackError::IpAddressError)?;
                    } else {
                        info!("Default gateway: None");
                        self.iface.routes_mut().remove_default_ipv4_route();
                    }

                    for (i, s) in config.dns_servers.iter().enumerate() {
                        let addr_octect = s.octets();
                        info!(
                            "DNS server {}:    {}.{}.{}.{}",
                            i, addr_octect[0], addr_octect[1], addr_octect[2], addr_octect[3]
                        );
                    }

                    break;
                }
                Some(dhcpv4::Event::Deconfigured) => {
                    if first_attempt == false {
                        info!("Lost DHCP configuration! Retrying…");
                    }
                    first_attempt = false;
                    self.iface.update_ip_addrs(|addrs| addrs.clear());
                    self.iface.routes_mut().remove_default_ipv4_route();
                }
            }

            if timestamp.total_millis() > max_timeout_msec {
                error!("DHCP Timeout");
                return Err(TcpStackError::Timeout);
            }

            let duration = self.iface.poll_delay(timestamp, &self.sockets);

            if let Some(dur) = duration {
                self.clock.delay_duration(dur)?;
            } else {
                self.clock.delay_millis(POLL_INTERFACE_TIMEOUT_MSEC)?;
            }
        }

        Ok(())
    }

    /// Send ping to server
    pub fn send_ping(&'a mut self, remote_ip: Ipv4Addr, count: u16) -> Result<(), TcpStackError> {
        const ECHO_IDENTIFER: u16 = 0x22;
        const IDLE_MAX_TIMEOUT_MS: i64 = 2000;

        let mut received: u16 = 0;
        let mut seq_no: u16 = 0;
        let mut echo_payload = [0xffu8; 40];
        let remote_addr: IpAddress = IpAddress::Ipv4(remote_ip);

        let icmp_rx_buffer = icmp::PacketBuffer::new(
            &mut self.icmp_storage.rx_meta[..],
            &mut self.icmp_storage.rx_buf[..],
        );
        let icmp_tx_buffer = icmp::PacketBuffer::new(
            &mut self.icmp_storage.tx_meta[..],
            &mut self.icmp_storage.tx_buf[..],
        );
        let icmp_socket = icmp::Socket::new(icmp_rx_buffer, icmp_tx_buffer);
        let icmp_handle = self.sockets.add(icmp_socket);
        let device_caps = self.device.capabilities();

        let ip = remote_ip.octets();

        let mut last_seen = self.clock.now()?;

        info!(
            "PING {}.{}.{}.{} with {} bytes of data:",
            ip[0],
            ip[1],
            ip[2],
            ip[3],
            echo_payload.len()
        );

        loop {
            self.iface
                .poll(self.clock.now()?, self.device, &mut self.sockets);

            let mut timestamp = self.clock.now()?; // reacquire

            let icmp_socket = self.sockets.get_mut::<icmp::Socket>(icmp_handle);
            if !icmp_socket.is_open() {
                icmp_socket
                    .bind(icmp::Endpoint::Ident(ECHO_IDENTIFER))
                    .map_err(|_| TcpStackError::SocketError)?;
            }

            if icmp_socket.can_send() && seq_no < count {
                NetworkEndian::write_i64(&mut echo_payload, timestamp.total_millis());

                let (icmp_repr, mut icmp_packet) = send_icmp_ping!(
                    Icmpv4Repr,
                    Icmpv4Packet,
                    ECHO_IDENTIFER,
                    seq_no,
                    echo_payload,
                    icmp_socket,
                    remote_addr
                );
                icmp_repr.emit(&mut icmp_packet, &device_caps.checksum);
                seq_no += 1;
                last_seen = timestamp;
            }

            if icmp_socket.can_recv() && seq_no > received {
                let (payload, _) = icmp_socket
                    .recv()
                    .map_err(|_| TcpStackError::ReadWriteError)?;
                let icmp_packet = Icmpv4Packet::new_checked(&payload)
                    .map_err(|_| TcpStackError::IcmpPacketError)?;
                let icmp_repr = Icmpv4Repr::parse(&icmp_packet, &device_caps.checksum)
                    .map_err(|_| TcpStackError::IcmpPacketError)?;

                match icmp_repr {
                    Icmpv4Repr::EchoReply {
                        seq_no: rx_seq,
                        data,
                        ..
                    } => {
                        let sent_ms = NetworkEndian::read_i64(data);
                        let rtt = timestamp.total_millis() - sent_ms;

                        info!(
                            "Reply from {} bytes from {}.{}.{}.{}: icmp_seq={}, time={}ms",
                            data.len(),
                            ip[0],
                            ip[1],
                            ip[2],
                            ip[3],
                            rx_seq,
                            rtt
                        );
                    }

                    Icmpv4Repr::EchoRequest { .. } => {
                        error!("Invalid Echo Request received.");
                        return Err(TcpStackError::InvalidResponse);
                    }

                    Icmpv4Repr::DstUnreachable { header, .. } => {
                        let ip = header.src_addr.octets();
                        info!(
                            "Reply from {}.{}.{}.{}: Destination host unreachable.",
                            ip[0], ip[1], ip[2], ip[3],
                        );
                    }
                    _ => {
                        info!("Request Timeout");
                    }
                }

                last_seen = timestamp;
                received += 1;
            }

            let idle_ms = timestamp.total_millis() - last_seen.total_millis();

            if (seq_no >= count && received >= count) || idle_ms > IDLE_MAX_TIMEOUT_MS {
                if idle_ms > IDLE_MAX_TIMEOUT_MS {
                    error!("Ping Timeout");
                    return Err(TcpStackError::Timeout);
                }

                break;
            }

            // Wait for new packet
            timestamp = self.clock.now()?;
            let duration = self.iface.poll_delay(timestamp, &self.sockets);

            if let Some(dur) = duration {
                self.clock.delay_duration(dur)?;
            } else {
                self.clock.delay_millis(POLL_INTERFACE_TIMEOUT_MSEC)?;
            }
        }

        let remote = remote_ip.octets();
        info!(
            "--- {}.{}.{}.{} ping statistics ---",
            remote[0], remote[1], remote[2], remote[3]
        );

        let loss = if seq_no == 0 {
            0.0
        } else {
            100.0 * (seq_no - received) as f64 / seq_no as f64
        };

        info!(
            "{} packets transmitted, {} received, {}% packet loss",
            seq_no, received, loss
        );

        Ok(())
    }
}
