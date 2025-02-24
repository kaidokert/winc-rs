use core::net::{IpAddr, SocketAddr};

use super::{debug, error, info};
use embedded_nal::nb::block;
use embedded_nal::TcpClientStack;
use iperf_data::{Cmds, SessionConfig, SessionResults, StreamResults};
use rand_core::RngCore;

mod iperf_data;

const DEFAULT_PORT: u16 = 5201;

pub enum Errors {
    TCP,
    UnexpectedResponse,
    JsonTooLarge,
}

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
    T::Error: embedded_nal::TcpError,
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

fn send_json<T, S>(stack: &mut T, mut control_socket: &mut S, out: &str) -> Result<usize, T::Error>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: embedded_nal::TcpError,
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
    T::Error: embedded_nal::TcpError,
{
    let mut jsonlen = [0; 4];
    block!(stack.receive(&mut control_socket, &mut jsonlen))?;
    let len = u32::from_be_bytes(jsonlen) as usize;
    if len > buffer.len() {
        return Err(Errors::JsonTooLarge);
    }
    let slice = &mut buffer[..len];

    info!("Incoming len {}", len);
    block!(stack.receive(&mut control_socket, slice))?;
    let json = core::str::from_utf8(slice).unwrap();

    Ok(json)
}

fn send_cmd<T, S>(stack: &mut T, mut control_socket: &mut S, cmd: Cmds) -> Result<usize, T::Error>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: embedded_nal::TcpError,
{
    let buf = [cmd as u8];
    block!(stack.send(&mut control_socket, &buf))
}

const MAX_BLOCK_LEN: usize = 1024;

pub fn iperf3_client<T, S>(
    stack: &mut T,
    server_addr: core::net::Ipv4Addr,
    port: Option<u16>,
    rng: &mut dyn RngCore,
) -> Result<(), Errors>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: embedded_nal::TcpError,
{
    let full_len = 1024_1000 * 20;
    let block_len = 256;
    assert!(block_len <= MAX_BLOCK_LEN);

    let mut control_socket = stack.socket()?;
    let remote = SocketAddr::new(IpAddr::V4(server_addr), port.unwrap_or(DEFAULT_PORT));
    block!(stack.connect(&mut control_socket, remote))?;
    info!("-----Socket connected-----");

    let cookie = make_cookie(rng);
    block!(stack.send(&mut control_socket, &cookie))?;
    debug!(
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
    debug!("-----Sent param exchange-----");

    read_control(stack, &mut control_socket, Cmds::CreateStreams)?;
    {
        let mut transport_socket = stack.socket()?;
        block!(stack.connect(&mut transport_socket, remote))?;
        block!(stack.send(&mut transport_socket, &cookie))?;
        debug!("-----Sent cookie to transport socket-----");
        read_control(stack, &mut control_socket, Cmds::TestStart)?;
        info!("-----Test started-----");
        read_control(stack, &mut control_socket, Cmds::TestRunning)?;
        debug!("-----Test running-----");
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
    debug!("-----Sending results----- {}", json);
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
        info!("Speed {} in Gb/s", speed / 1_000_000_000.0);
    } else if speed > 1_000_000.0 {
        info!("Speed {} in Mb/s", speed / 1000_000.0);
    } else if speed > 1000.0 {
        info!("Speed {} in kb/s", speed / 1000.0);
    } else {
        info!("Speed {} in bytes/s", speed);
    }

    send_cmd(stack, &mut control_socket, Cmds::IperfDone)?;
    Ok(())
}
