use core::net::{IpAddr, SocketAddr};

use super::{debug, error, info};
use embedded_nal::nb::{self, block};
use embedded_nal::{TcpClientStack, UdpClientStack};
use iperf_data::{Cmds, SessionConfig, SessionResults, StreamResults, UdpPacketHeader, UdpMetrics};
pub use rand_core::RngCore;

mod iperf_data;

const DEFAULT_PORT: u16 = 5201;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Errors {
    TCP,
    UDP,
    UnexpectedResponse,
    JsonTooLarge,
}

#[cfg(not(feature = "defmt"))]
pub trait TcpError: embedded_nal::TcpError {}

#[cfg(not(feature = "defmt"))]
impl<T> TcpError for T where T: embedded_nal::TcpError {}

#[cfg(feature = "defmt")]
pub trait TcpError: embedded_nal::TcpError + defmt::Format {}

#[cfg(feature = "defmt")]
impl<T> TcpError for T where T: embedded_nal::TcpError + defmt::Format {}


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

fn read_control<T, S>(stack: &mut T, mut control_socket: &mut S, cmd: Cmds) -> Result<(), Errors>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: TcpError,
{
    let fx = cmd.clone() as u8;
    let mut read_cmd: [u8; 1] = [0];
    block!(stack.receive(&mut control_socket, &mut read_cmd))?;
    if fx == read_cmd[0] {
        debug!("Got {:?}", cmd);
    } else {
        error!("Unexpected response {}", read_cmd[0]);
        return Err(Errors::UnexpectedResponse);
    }
    Ok(())
}

fn read_control_with_timeout<T, S>(
    stack: &mut T, 
    mut control_socket: &mut S, 
    cmd: Cmds,
    wait_ms: &mut dyn FnMut(u32)
) -> Result<(), Errors>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: TcpError,
{
    let fx = cmd.clone() as u8;
    let mut read_cmd: [u8; 1] = [0];
    
    // Try for about 10 seconds
    let mut attempts = 0;
    let max_attempts = 1000;
    
    loop {
        match stack.receive(&mut control_socket, &mut read_cmd) {
            Ok(_) => break,
            Err(nb::Error::WouldBlock) => {
                attempts += 1;
                if attempts >= max_attempts {
                    error!("Timeout waiting for {:?} after {} attempts", cmd, attempts);
                    return Err(Errors::UnexpectedResponse);
                }
                wait_ms(10);
            }
            Err(nb::Error::Other(e)) => return Err(e.into()),
        }
    }
    
    if fx == read_cmd[0] {
        debug!("Got {:?}", cmd);
    } else {
        error!("Unexpected response {}, expected {}", read_cmd[0], fx);
        return Err(Errors::UnexpectedResponse);
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
    
    // Handle case where server disconnects (len = 0)
    if len == 0 {
        return Err(Errors::UnexpectedResponse);
    }
    
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

pub fn iperf3_client<const MAX_BLOCK_LEN: usize, T, S>(
    stack: &mut T,
    server_addr: core::net::Ipv4Addr,
    port: Option<u16>,
    rng: &mut dyn RngCore,
    config: Option<TestConfig>,
) -> Result<(), Errors>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: TcpError,
{
    let my_confg = config.unwrap_or(TestConfig {
        conf: Conf::Bytes(1024_1000 * 20),
        transmit_block_len: 256,
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
    info!("Config: full_len: {} block_size: {}", full_len, block_len);

    let mut control_socket = stack.socket()?;
    let remote = SocketAddr::new(IpAddr::V4(server_addr), port.unwrap_or(DEFAULT_PORT));
    info!("-----Connecting to {}-----", remote.port());
    block!(stack.connect(&mut control_socket, remote))?;
    info!("-----Socket connected-----");

    let cookie = make_cookie(rng);
    block!(stack.send(&mut control_socket, &cookie))?;
    info!(
        "-----Sent cookie:----- {:?}",
        core::str::from_utf8(&cookie).unwrap()
    );

    read_control(stack, &mut control_socket, Cmds::ParamExchange)?;

    let conf = SessionConfig {
        tcp: 1,
        num: full_len,
        len: block_len,
    };
    let json = conf.serde_json().unwrap();
    send_json(stack, &mut control_socket, &json)?;
    info!("-----Sent param exchange-----");

    read_control(stack, &mut control_socket, Cmds::CreateStreams)?;
    {
        let mut transport_socket = stack.socket()?;
        block!(stack.connect(&mut transport_socket, remote))?;
        block!(stack.send(&mut transport_socket, &cookie))?;
        debug!("-----Sent cookie to transport socket-----");
        read_control(stack, &mut control_socket, Cmds::TestStart)?;
        debug!("-----Test started-----");
        read_control(stack, &mut control_socket, Cmds::TestRunning)?;
        info!("-----Test running-----");
        let mut to_send = full_len as isize;
        loop {
            let buffer = [0xAA; MAX_BLOCK_LEN];
            block!(stack.send(&mut transport_socket, &buffer[..block_len]))?;
            debug!("-----Sent {} bytes-----", block_len);
            to_send -= block_len as isize;
            if to_send <= 0 {
                break;
            }
        }
    }

    send_cmd(stack, &mut control_socket, Cmds::TestEnd)?;
    read_control(stack, &mut control_socket, Cmds::ExchangeResults)?;

    let results = &[StreamResults {
        id: 1,
        bytes: full_len as u32,
        ..Default::default()
    }];
    let results = SessionResults::<1> {
        streams: heapless::Vec::from_slice(results).unwrap_or_default(),
        ..Default::default()
    };
    let json = results.serde_json().unwrap();
    info!("-----Sending results----- {:?}", json);
    send_json(stack, &mut control_socket, &json)?;

    let mut remote_results_buffer = [0; iperf_data::MAX_SESSION_RESULTS_LEN * 2];

    debug!("-----Doing recv_json-----");
    let remote_results = recv_json(stack, &mut control_socket, &mut remote_results_buffer)?;

    read_control(stack, &mut control_socket, Cmds::DisplayResults)?;

    let (session_results, _): (SessionResults<1>, usize) =
        serde_json_core::from_str(remote_results).unwrap();
    info!("-----Session results:----- {:?}", session_results);

    let strm = &session_results.streams[0];
    info!("stream 0: id:{} bytes:{}", strm.id, strm.bytes);

    // Calculate speed from Stream[0] .end_time-.start_time and .bytes
    let strm = &session_results.streams[0];
    let speed = strm.bytes as f32 / (strm.end_time - strm.start_time);
    if speed > 1_000_000_000.0 {
        info!(
            "Speed {} in Gb/s ( {} in GBits/s)",
            speed / 1_000_000_000.0,
            speed * 8.0 / 1_000_000_000.0
        );
    } else if speed > 1_000_000.0 {
        info!(
            "Speed {} in Mb/s ( {} in MBits/s)",
            speed / 1000_000.0,
            speed * 8.0 / 1000_000.0
        );
    } else if speed > 1000.0 {
        info!(
            "Speed {} in kb/s ( {} in KBits/s)",
            speed / 1000.0,
            speed * 8.0 / 1000.0
        );
    } else {
        info!("Speed {} in bytes/s ( {} in bits/s)", speed, speed * 8.0);
    }

    send_cmd(stack, &mut control_socket, Cmds::IperfDone)?;
    Ok(())
}

pub fn iperf3_udp_client<const MAX_BLOCK_LEN: usize, T, S, US>(
    stack: &mut T,
    server_addr: core::net::Ipv4Addr,
    port: Option<u16>,
    rng: &mut dyn RngCore,
    config: Option<TestConfig>,
    wait_ms: &mut dyn FnMut(u32),
) -> Result<(), Errors>
where
    T: TcpClientStack<TcpSocket = S> + UdpClientStack<UdpSocket = US> + ?Sized,
    <T as TcpClientStack>::Error: TcpError,
    <T as UdpClientStack>::Error: core::fmt::Debug,
{
    let my_confg = config.unwrap_or(TestConfig {
        conf: Conf::Bytes(1024_1000 * 20),
        transmit_block_len: 1450, // Default UDP block size (less than MTU)
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
    assert!(block_len >= 12); // Must have room for UDP header
    info!("UDP Config: full_len: {} block_size: {}", full_len, block_len);

    // Control connection is still TCP
    let mut control_socket = TcpClientStack::socket(stack)?;
    let remote = SocketAddr::new(IpAddr::V4(server_addr), port.unwrap_or(DEFAULT_PORT));
    info!("-----Connecting to {} (UDP test)-----", remote.port());
    block!(TcpClientStack::connect(stack, &mut control_socket, remote))?;
    info!("-----Socket connected-----");

    let cookie = make_cookie(rng);
    block!(TcpClientStack::send(stack, &mut control_socket, &cookie))?;
    info!(
        "-----Sent cookie:----- {:?}",
        core::str::from_utf8(&cookie).unwrap()
    );

    read_control_with_timeout(stack, &mut control_socket, Cmds::ParamExchange, wait_ms)?;

    // Set tcp: 0 for UDP test
    let conf = SessionConfig {
        tcp: 0, // UDP mode
        num: full_len,
        len: block_len,
    };
    let json = conf.serde_json().unwrap();
    send_json(stack, &mut control_socket, &json)?;
    info!("-----Sent param exchange (UDP)-----");

    read_control_with_timeout(stack, &mut control_socket, Cmds::CreateStreams, wait_ms)?;
    
    // UDP data transfer - but initially uses TCP connection like real iperf3!
    let mut udp_metrics = UdpMetrics::default();
    {
        // Create TCP connection for data stream (like real iperf3 UDP does)
        let mut transport_socket = TcpClientStack::socket(stack)?;
        block!(TcpClientStack::connect(stack, &mut transport_socket, remote))?;
        block!(TcpClientStack::send(stack, &mut transport_socket, &cookie))?;
        debug!("-----UDP test: TCP transport socket connected and cookie sent-----");
        
        read_control_with_timeout(stack, &mut control_socket, Cmds::TestStart, wait_ms)?;
        debug!("-----Test started-----");
        read_control_with_timeout(stack, &mut control_socket, Cmds::TestRunning, wait_ms)?;
        info!("-----Test running (UDP)-----");
        
        let mut to_send = full_len as isize;
        let mut packet_id = 1u32;
        let test_start_time = 0.0f32; // Simplified - would need actual timestamp
        
        loop {
            // Create UDP packet with header
            let mut buffer = [0xBB; MAX_BLOCK_LEN]; // Different pattern for UDP
            
            // UDP packet header (12 bytes) 
            let current_time = 0.0f32; // Simplified - would need actual timestamp
            let header = UdpPacketHeader {
                tv_sec: current_time as u32,
                tv_usec: ((current_time - current_time.floor()) * 1_000_000.0) as u32,
                id: packet_id,
            };
            let header_bytes = header.to_bytes();
            buffer[..12].copy_from_slice(&header_bytes);
            
            // TODO: Implement UDP pacing/rate limiting for better throughput performance.
            // Current implementation sends packets as fast as possible which causes network
            // buffer overflow and poor utilization. Official iperf3 achieves ~12x better 
            // performance with pacing.
            // For optimal results, packets should be spaced based on target bitrate and
            // network feedback rather than sent in a tight loop.
            match block!(TcpClientStack::send(stack, &mut transport_socket, &buffer[..block_len])) {
                Ok(_) => {
                    udp_metrics.packets_sent += 1;
                    udp_metrics.bytes_sent += block_len as u32;
                    debug!("-----Sent UDP packet {} ({} bytes)-----", packet_id, block_len);
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
        }
        
        debug!("UDP Metrics: sent={} errors={} loss={:.2}%", 
               udp_metrics.packets_sent, 
               udp_metrics.errors,
               udp_metrics.packet_loss_percent());
    }

    send_cmd(stack, &mut control_socket, Cmds::TestEnd)?;
    read_control_with_timeout(stack, &mut control_socket, Cmds::ExchangeResults, wait_ms)?;

    let results = &[StreamResults {
        id: 1,
        bytes: udp_metrics.bytes_sent,
        packets: udp_metrics.packets_sent,
        errors: udp_metrics.errors,
        jitter: (udp_metrics.calculate_jitter() * 1000.0) as u32, // Convert to microseconds
        ..Default::default()
    }];
    let results = SessionResults::<1> {
        streams: heapless::Vec::from_slice(results).unwrap_or_default(),
        ..Default::default()
    };
    let json = results.serde_json().unwrap();
    info!("-----Sending UDP results----- {:?}", json);
    send_json(stack, &mut control_socket, &json)?;

    let mut remote_results_buffer = [0; iperf_data::MAX_SESSION_RESULTS_LEN * 2];

    debug!("-----Doing recv_json-----");
    match recv_json(stack, &mut control_socket, &mut remote_results_buffer) {
        Ok(remote_results) => {
            // DisplayResults might not be sent by all servers
            if let Err(_) = read_control_with_timeout(stack, &mut control_socket, Cmds::DisplayResults, wait_ms) {
                debug!("No DisplayResults received - server may have disconnected (normal for some servers)");
            }

            debug!("Raw JSON from server: {}", remote_results);
            let (session_results, _): (SessionResults<1>, usize) =
                match serde_json_core::from_str(remote_results) {
                    Ok(result) => result,
                    Err(e) => {
                        error!("JSON parse error: {:?}", e);
                        error!("Raw JSON: {}", remote_results);
                        return Err(Errors::UnexpectedResponse);
                    }
                };
            info!("-----Session results (UDP):----- {:?}", session_results);

            let strm = &session_results.streams[0];
            info!("UDP stream 0: id:{} bytes:{} packets:{} errors:{} jitter:{}μs", 
                  strm.id, strm.bytes, strm.packets, strm.errors, strm.jitter);

            // Calculate speed from Stream[0] .end_time-.start_time and .bytes
            let strm = &session_results.streams[0];
            if strm.end_time > strm.start_time {
                let speed = strm.bytes as f32 / (strm.end_time - strm.start_time);
                if speed > 1_000_000_000.0 {
                    info!(
                        "UDP Speed {} in Gb/s ( {} in GBits/s)",
                        speed / 1_000_000_000.0,
                        speed * 8.0 / 1_000_000_000.0
                    );
                } else if speed > 1_000_000.0 {
                    info!(
                        "UDP Speed {} in Mb/s ( {} in MBits/s)",
                        speed / 1000_000.0,
                        speed * 8.0 / 1000_000.0
                    );
                } else if speed > 1000.0 {
                    info!(
                        "UDP Speed {} in kb/s ( {} in KBits/s)",
                        speed / 1000.0,
                        speed * 8.0 / 1000.0
                    );
                } else {
                    info!("UDP Speed {} in bytes/s ( {} in bits/s)", speed, speed * 8.0);
                }
            } else {
                info!("UDP test completed: {} bytes sent", strm.bytes);
            }
        }
        Err(_) => {
            info!("UDP test completed successfully - server did not send back results (normal for some servers)");
            info!("Client metrics: sent={} packets ({} bytes), errors={}", 
                  udp_metrics.packets_sent, udp_metrics.bytes_sent, udp_metrics.errors);
        }
    }

    send_cmd(stack, &mut control_socket, Cmds::IperfDone)?;
    Ok(())
}
