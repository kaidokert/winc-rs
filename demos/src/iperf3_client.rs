use core::net::{IpAddr, SocketAddr};

use super::{debug, error, info};
use embedded_nal::nb::{self, block};
use embedded_nal::{TcpClientStack, UdpClientStack};
use iperf_data::{
    Cmds, SessionConfig, SessionResults, StreamResults, UdpMetrics, UdpPacketHeader,
    UdpSessionConfig,
};
pub use rand_core::RngCore;

mod iperf_data;

macro_rules! block_timeout {
    ($e:expr, $delay:expr, $max_attempts:expr) => {{
        let mut attempts = 0;
        loop {
            #[allow(unreachable_patterns)]
            match $e {
                Err(nb::Error::Other(e)) => {
                    #[allow(unreachable_code)]
                    break Err(nb::Error::Other(e));
                }
                Err(nb::Error::WouldBlock) => {
                    attempts += 1;
                    if attempts > $max_attempts {
                        break Err(nb::Error::WouldBlock);
                    }
                    $delay(10);
                }
                Ok(x) => break Ok(x),
            }
        }
    }};
}

const DEFAULT_PORT: u16 = 5201;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Errors {
    TCP,
    UDP,
    UnexpectedResponse,
    JsonTooLarge,
    Timeout,
}

#[cfg(not(feature = "defmt"))]
pub trait TcpError: embedded_nal::TcpError {}

#[cfg(not(feature = "defmt"))]
impl<T> TcpError for T where T: embedded_nal::TcpError {}

#[cfg(feature = "defmt")]
pub trait TcpError: embedded_nal::TcpError + defmt::Format {}

#[cfg(feature = "defmt")]
impl<T> TcpError for T where T: embedded_nal::TcpError + defmt::Format {}

#[cfg(not(feature = "defmt"))]
pub trait UdpError: core::fmt::Debug {}

#[cfg(not(feature = "defmt"))]
impl<T> UdpError for T where T: core::fmt::Debug {}

#[cfg(feature = "defmt")]
pub trait UdpError: core::fmt::Debug + defmt::Format {}

#[cfg(feature = "defmt")]
impl<T> UdpError for T where T: core::fmt::Debug + defmt::Format {}

impl<T> From<T> for Errors
where
    T: embedded_nal::TcpError,
{
    // TODO: Discards inner error for now
    fn from(_err: T) -> Self {
        Errors::TCP
    }
}

fn make_cookie(gen: &mut dyn rand_core::RngCore) -> [u8; 37] {
    let mut bytes = [0; 37];
    gen.fill_bytes(&mut bytes);
    // could be any bytes, but we only send alphabet characters
    bytes.iter_mut().for_each(|b| *b = b'a' + (*b % 26));
    bytes
}

fn format_speed(bytes_per_second: f32) -> (f32, &'static str, f32, &'static str) {
    let mut speed = bytes_per_second;
    let suffixes = ["bytes", "KB", "MB", "GB"];
    let mut suffix_index = 0;

    while speed >= 1000.0 && suffix_index < suffixes.len() - 1 {
        speed /= 1000.0;
        suffix_index += 1;
    }

    let bits_speed = speed * 8.0;
    let bits_suffix = if suffix_index == 0 {
        "bits"
    } else {
        match suffix_index {
            1 => "Kbits",
            2 => "Mbits",
            3 => "Gbits",
            _ => "bits",
        }
    };

    (speed, suffixes[suffix_index], bits_speed, bits_suffix)
}

fn read_control<T, S>(
    stack: &mut T,
    mut control_socket: &mut S,
    cmd: Cmds,
    delay_ms: &mut impl FnMut(u32),
) -> Result<(), Errors>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: TcpError,
{
    let fx = cmd.clone() as u8;
    let mut read_cmd: [u8; 1] = [0];
    debug!("Waiting for control command: {:?} ({})", cmd, fx);

    match block_timeout!(
        stack.receive(&mut control_socket, &mut read_cmd),
        delay_ms,
        50
    ) {
        Ok(_) => {
            debug!("Received control byte: {}", read_cmd[0]);
            if fx == read_cmd[0] {
                debug!("Got expected {:?}", cmd);
            } else {
                error!(
                    "Unexpected response: expected {} ({:?}), got {}",
                    fx, cmd, read_cmd[0]
                );
                return Err(Errors::UnexpectedResponse);
            }
        }
        Err(e) => match e {
            nb::Error::WouldBlock => {
                error!("Timeout waiting for control command {:?}", cmd);
                return Err(Errors::Timeout);
            }
            nb::Error::Other(_) => {
                error!("Failed to receive control command {:?}", cmd);
                return Err(Errors::TCP);
            }
        },
    }
    Ok(())
}

fn send_json<T, S>(stack: &mut T, mut control_socket: &mut S, out: &str) -> Result<usize, T::Error>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: TcpError,
{
    let jsonbytes = out.as_bytes();
    let jsonlen = (jsonbytes.len() as u32).to_be_bytes();
    block!(stack.send(&mut control_socket, &jsonlen))?;
    block!(stack.send(&mut control_socket, jsonbytes))
}

fn recv_json<'a, T, S>(
    stack: &mut T,
    mut control_socket: &mut S,
    buffer: &'a mut [u8],
) -> Result<&'a str, Errors>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: TcpError,
{
    let mut jsonlen = [0; 4];
    block!(stack.receive(&mut control_socket, &mut jsonlen))?;
    let len = u32::from_be_bytes(jsonlen) as usize;

    info!("Incoming len {}", len);

    if len > buffer.len() {
        return Err(Errors::JsonTooLarge);
    }
    let slice = &mut buffer[..len];

    block!(stack.receive(&mut control_socket, slice))?;
    let json = core::str::from_utf8(slice).unwrap();

    Ok(json)
}

fn send_cmd<T, S>(stack: &mut T, mut control_socket: &mut S, cmd: Cmds) -> Result<usize, T::Error>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: TcpError,
{
    let buf = [cmd as u8];
    block!(stack.send(&mut control_socket, &buf))
}

pub enum Conf {
    Time(usize),
    Bytes(usize),
    Blocks(usize),
}

pub struct TestConfig {
    pub conf: Conf,
    pub transmit_block_len: usize,
}

pub fn iperf3_client<const MAX_BLOCK_LEN: usize, T, S, US>(
    stack: &mut T,
    server_addr: core::net::Ipv4Addr,
    port: Option<u16>,
    rng: &mut dyn RngCore,
    config: Option<TestConfig>,
    use_udp: bool,
    delay_ms: &mut impl FnMut(u32),
) -> Result<(), Errors>
where
    T: TcpClientStack<TcpSocket = S> + UdpClientStack<UdpSocket = US> + ?Sized,
    <T as TcpClientStack>::Error: TcpError,
    <T as UdpClientStack>::Error: UdpError,
{
    let my_confg = config.unwrap_or(TestConfig {
        conf: Conf::Bytes(1024_1000 * 20),
        transmit_block_len: if use_udp { 1450 } else { 256 }, // Different defaults for UDP vs TCP
    });

    let full_len = match my_confg.conf {
        Conf::Time(_time) => {
            todo!()
        }
        Conf::Bytes(bytes) => bytes,
        Conf::Blocks(blocks) => blocks * my_confg.transmit_block_len,
    };
    let block_len = my_confg.transmit_block_len;

    assert!(block_len <= MAX_BLOCK_LEN);
    if use_udp {
        assert!(block_len >= 12); // Must have room for UDP header
    }
    let protocol_name = if use_udp { "UDP" } else { "TCP" };
    info!(
        "{} Config: full_len: {} block_size: {}",
        protocol_name, full_len, block_len
    );

    // Control connection is always TCP
    let mut control_socket = TcpClientStack::socket(stack)?;
    let remote = SocketAddr::new(IpAddr::V4(server_addr), port.unwrap_or(DEFAULT_PORT));
    info!(
        "-----Connecting to {}.{}.{}.{}:{} ({} test)-----",
        server_addr.octets()[0],
        server_addr.octets()[1],
        server_addr.octets()[2],
        server_addr.octets()[3],
        remote.port(),
        protocol_name
    );
    block_timeout!(
        TcpClientStack::connect(stack, &mut control_socket, remote),
        delay_ms,
        300
    )
    .map_err(|e| match e {
        nb::Error::WouldBlock => {
            error!("Timeout connecting to TCP control socket");
            Errors::Timeout
        }
        nb::Error::Other(_) => {
            error!("Failed to connect to TCP control socket");
            Errors::TCP
        }
    })?;
    info!("-----Socket connected-----");

    let cookie = make_cookie(rng);
    block!(TcpClientStack::send(stack, &mut control_socket, &cookie))?;
    info!(
        "-----Sent cookie:----- {:?}",
        core::str::from_utf8(&cookie).unwrap()
    );

    read_control(stack, &mut control_socket, Cmds::ParamExchange, delay_ms)?;

    // Create protocol-specific configuration
    if use_udp {
        let udp_conf = UdpSessionConfig {
            udp: true,
            omit: 0,
            time: 3, // Default 3 seconds - could be made configurable
            num: 0,  // When using time, num should be 0
            blockcount: 0,
            parallel: 1,
            len: block_len as u64,
            bandwidth: 1048576, // 1 Mbps default
            pacing_timer: 1000,
            client_version: heapless::String::try_from("3.19").unwrap(),
        };
        let json = udp_conf.serde_json().unwrap();
        send_json(stack, &mut control_socket, &json)?;
    } else {
        let tcp_conf = SessionConfig {
            tcp: 1,
            num: full_len as u64,
            len: block_len as u64,
        };
        let json = tcp_conf.serde_json().unwrap();
        send_json(stack, &mut control_socket, &json)?;
    }
    info!("-----Sent param exchange ({})-----", protocol_name);

    read_control(stack, &mut control_socket, Cmds::CreateStreams, delay_ms)?;
    debug!("-----Received CreateStreams command-----");

    // Create data connection immediately after CreateStreams as iperf3 expects
    let mut udp_metrics = UdpMetrics::default();
    let mut udp_socket_option: Option<US> = None;
    let mut tcp_socket_option: Option<S> = None;

    if use_udp {
        debug!("-----Creating UDP data socket-----");
        let mut udp_socket = UdpClientStack::socket(stack).map_err(|e| {
            error!("Failed to create UDP socket: {:?}", e);
            Errors::UDP
        })?;
        debug!("-----UDP socket created, connecting-----");
        UdpClientStack::connect(stack, &mut udp_socket, remote).map_err(|e| {
            error!("Failed to connect UDP socket: {:?}", e);
            Errors::UDP
        })?;
        debug!("-----UDP data socket connected-----");

        // Send UDP connect message as per iperf3 protocol
        let udp_connect_msg: [u8; 4] = 0x36373839u32.to_be_bytes(); // "6789"
        debug!("-----Sending UDP connect message-----");
        block!(UdpClientStack::send(
            stack,
            &mut udp_socket,
            &udp_connect_msg
        ))
        .map_err(|e| {
            error!("Failed to send UDP connect message: {:?}", e);
            Errors::UDP
        })?;

        // Wait for UDP connect reply
        let mut reply_buf = [0u8; 4];
        debug!("-----Waiting for UDP connect reply-----");
        let (reply_len, _) = block_timeout!(
            UdpClientStack::receive(stack, &mut udp_socket, &mut reply_buf),
            delay_ms,
            50
        )
        .map_err(|e| match e {
            nb::Error::WouldBlock => {
                error!("Timeout waiting for UDP connect reply");
                Errors::Timeout
            }
            nb::Error::Other(_) => {
                error!("Failed to receive UDP connect reply");
                Errors::UDP
            }
        })?;

        if reply_len == 4 {
            let reply_value = u32::from_be_bytes(reply_buf);
            debug!(
                "-----Received UDP connect reply: 0x{:08x}-----",
                reply_value
            );
            if reply_value == 0x39383736 {
                // "9876" - expected reply
                debug!("-----UDP handshake completed with proper reply-----");
            } else if reply_value == 0x36373839 {
                // "6789" - echo of our message
                debug!("-----UDP handshake completed (received echo)-----");
            } else {
                debug!(
                    "-----UDP handshake completed with unexpected reply: 0x{:08x}-----",
                    reply_value
                );
            }
        } else {
            debug!(
                "-----UDP connect reply length: {} (continuing anyway)-----",
                reply_len
            );
        }

        // Store the UDP socket for reuse in data transfer
        udp_socket_option = Some(udp_socket);
    } else {
        let mut transport_socket = TcpClientStack::socket(stack)?;
        block!(TcpClientStack::connect(
            stack,
            &mut transport_socket,
            remote
        ))?;
        block!(TcpClientStack::send(stack, &mut transport_socket, &cookie))?;
        debug!("-----TCP data socket connected and cookie sent-----");

        // Store the TCP socket for reuse in data transfer
        tcp_socket_option = Some(transport_socket);
    }

    read_control(stack, &mut control_socket, Cmds::TestStart, delay_ms)?;
    debug!("-----Test started-----");
    read_control(stack, &mut control_socket, Cmds::TestRunning, delay_ms)?;
    info!("-----Test running ({})-----", protocol_name);

    let mut to_send = full_len as isize;
    let mut packet_id = 1u32;

    if use_udp {
        // UDP data transfer using the same socket from handshake
        let mut udp_socket = udp_socket_option.unwrap();

        // TODO: Implement UDP pacing/rate limiting for better throughput performance.
        // Current implementation sends packets as fast as possible which causes network
        // buffer overflow and poor utilization. Official iperf3 achieves ~12x better
        // performance (197 Mbps vs our 16 Mbps) through intelligent pacing algorithms.
        // For optimal results, packets should be spaced based on target bitrate and
        // network feedback rather than sent in a tight loop.

        // Allocate buffer once outside the loop to reduce stack pressure
        let mut buffer = [0xBB; MAX_BLOCK_LEN]; // Different pattern for UDP

        loop {
            // UDP packet header (12 bytes)
            let current_time = 0.0f32; // Simplified - would need actual timestamp
            let header = UdpPacketHeader {
                tv_sec: current_time as u32,
                tv_usec: 0, // Simplified - fractional seconds would be computed here
                id: packet_id as i32,
            };
            let header_bytes = header.to_bytes();
            buffer[..12].copy_from_slice(&header_bytes);

            match block!(UdpClientStack::send(
                stack,
                &mut udp_socket,
                &buffer[..block_len]
            )) {
                Ok(_) => {
                    udp_metrics.packets_sent += 1;
                    udp_metrics.bytes_sent += block_len as u64;
                    debug!(
                        "-----Sent UDP packet {} ({} bytes)-----",
                        packet_id, block_len
                    );
                }
                Err(_) => {
                    udp_metrics.errors += 1;
                    debug!("-----Failed to send UDP packet {}-----", packet_id);
                }
            }

            packet_id += 1;
            to_send -= block_len as isize;
            if to_send <= 0 {
                break;
            }

            // Simple pacing to prevent flooding the network stack
            delay_ms(1);
        }
    } else {
        // TCP data transfer using the same socket from handshake
        let mut transport_socket = tcp_socket_option.unwrap();

        loop {
            let buffer = [0xAA; MAX_BLOCK_LEN];
            block!(TcpClientStack::send(
                stack,
                &mut transport_socket,
                &buffer[..block_len]
            ))?;
            debug!("-----Sent {} bytes-----", block_len);
            to_send -= block_len as isize;
            if to_send <= 0 {
                break;
            }
        }

        debug!("-----TCP data transfer completed-----");
    }

    send_cmd(stack, &mut control_socket, Cmds::TestEnd)?;
    read_control(stack, &mut control_socket, Cmds::ExchangeResults, delay_ms)?;

    // Create results based on protocol
    let results = if use_udp {
        &[StreamResults {
            id: 1,
            bytes: udp_metrics.bytes_sent,
            packets: udp_metrics.packets_sent,
            errors: udp_metrics.errors,
            jitter: 0.0, // Convert to microseconds
            ..Default::default()
        }][..]
    } else {
        &[StreamResults {
            id: 1,
            bytes: full_len as u64,
            ..Default::default()
        }][..]
    };

    let results = SessionResults::<1> {
        streams: heapless::Vec::from_slice(results).unwrap_or_default(),
        ..Default::default()
    };
    let json = results.serde_json().unwrap();
    info!("-----Sending {} results----- {:?}", protocol_name, json);
    send_json(stack, &mut control_socket, &json)?;

    let mut remote_results_buffer = [0; iperf_data::MAX_SESSION_RESULTS_LEN * 2];

    debug!("-----Doing recv_json-----");
    match recv_json(stack, &mut control_socket, &mut remote_results_buffer) {
        Ok(remote_results) => {
            read_control(stack, &mut control_socket, Cmds::DisplayResults, delay_ms)?;

            let (session_results, _): (SessionResults<1>, usize) =
                match serde_json_core::from_str(remote_results) {
                    Ok(result) => result,
                    Err(_e) => {
                        error!("JSON parse error");
                        error!("Raw JSON: {}", remote_results);
                        return Err(Errors::UnexpectedResponse);
                    }
                };
            info!(
                "-----Session results ({}):----- {:?}",
                protocol_name, session_results
            );

            let strm = &session_results.streams[0];
            if use_udp {
                info!(
                    "{} stream 0: id:{} bytes:{} packets:{} errors:{} jitter:{}μs",
                    protocol_name, strm.id, strm.bytes, strm.packets, strm.errors, strm.jitter
                );
            } else {
                info!(
                    "{} stream 0: id:{} bytes:{}",
                    protocol_name, strm.id, strm.bytes
                );
            }

            // Calculate speed from Stream[0] .end_time-.start_time and .bytes
            let strm = &session_results.streams[0];
            if strm.end_time > strm.start_time {
                let bytes_per_second = strm.bytes as f32 / (strm.end_time - strm.start_time);
                let (speed, bytes_suffix, bits_speed, bits_suffix) = format_speed(bytes_per_second);
                info!(
                    "{} Speed {} {}/s ({} {}/s)",
                    protocol_name, speed, bytes_suffix, bits_speed, bits_suffix
                );
            } else {
                info!(
                    "{} test completed: {} bytes sent",
                    protocol_name, strm.bytes
                );
            }
        }
        Err(_) => {
            if use_udp {
                info!("{} test completed successfully - server did not send back results (normal for some servers)", protocol_name);
                info!(
                    "Client metrics: sent={} packets ({} bytes), errors={}",
                    udp_metrics.packets_sent, udp_metrics.bytes_sent, udp_metrics.errors
                );
            } else {
                info!(
                    "{} test completed successfully - server did not send back results",
                    protocol_name
                );
            }
        }
    }

    send_cmd(stack, &mut control_socket, Cmds::IperfDone)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_speed_bytes() {
        let (speed, bytes_suffix, bits_speed, bits_suffix) = format_speed(500.0);
        assert_eq!(speed, 500.0);
        assert_eq!(bytes_suffix, "bytes");
        assert_eq!(bits_speed, 4000.0);
        assert_eq!(bits_suffix, "bits");
    }

    #[test]
    fn test_format_speed_kb() {
        let (speed, bytes_suffix, bits_speed, bits_suffix) = format_speed(1500.0);
        assert_eq!(speed, 1.5);
        assert_eq!(bytes_suffix, "KB");
        assert_eq!(bits_speed, 12.0);
        assert_eq!(bits_suffix, "Kbits");
    }

    #[test]
    fn test_format_speed_mb() {
        let (speed, bytes_suffix, bits_speed, bits_suffix) = format_speed(2_500_000.0);
        assert_eq!(speed, 2.5);
        assert_eq!(bytes_suffix, "MB");
        assert_eq!(bits_speed, 20.0);
        assert_eq!(bits_suffix, "Mbits");
    }

    #[test]
    fn test_format_speed_gb() {
        let (speed, bytes_suffix, bits_speed, bits_suffix) = format_speed(1_500_000_000.0);
        assert_eq!(speed, 1.5);
        assert_eq!(bytes_suffix, "GB");
        assert_eq!(bits_speed, 12.0);
        assert_eq!(bits_suffix, "Gbits");
    }

    #[test]
    fn test_format_speed_exact_boundaries() {
        // Test exactly 1000 bytes
        let (speed, bytes_suffix, _, _) = format_speed(1000.0);
        assert_eq!(speed, 1.0);
        assert_eq!(bytes_suffix, "KB");

        // Test exactly 1 MB
        let (speed, bytes_suffix, _, _) = format_speed(1_000_000.0);
        assert_eq!(speed, 1.0);
        assert_eq!(bytes_suffix, "MB");

        // Test exactly 1 GB
        let (speed, bytes_suffix, _, _) = format_speed(1_000_000_000.0);
        assert_eq!(speed, 1.0);
        assert_eq!(bytes_suffix, "GB");
    }

    #[test]
    fn test_format_speed_very_large() {
        // Test very large value - should cap at GB
        let (speed, bytes_suffix, bits_speed, bits_suffix) = format_speed(5_000_000_000_000.0);
        assert_eq!(speed, 5000.0);
        assert_eq!(bytes_suffix, "GB");
        assert_eq!(bits_speed, 40000.0);
        assert_eq!(bits_suffix, "Gbits");
    }
}
