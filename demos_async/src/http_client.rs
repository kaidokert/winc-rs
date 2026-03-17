use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use embedded_io_async::{Read, Write};
use embedded_nal_async::TcpConnect;

#[cfg(feature = "defmt")]
use defmt::info;
#[cfg(feature = "log")]
use log::info;

// Max length of Hostname
pub const MAX_HOSTNAME_LEN: usize = 64;

pub async fn run_http_client<T: TcpConnect>(
    stack: &mut T,
    server_ip: Ipv4Addr,
    server_port: u16,
    hostname: Option<&[u8; MAX_HOSTNAME_LEN]>,
) -> Result<(), T::Error> {
    let remote = SocketAddr::new(IpAddr::V4(server_ip), server_port);
    info!(
        "-----connecting to ----- {}.{}.{}.{} port {}",
        server_ip.octets()[0],
        server_ip.octets()[1],
        server_ip.octets()[2],
        server_ip.octets()[3],
        server_port
    );
    let mut tcp_client = stack.connect(remote).await?;
    info!("-----Socket connected-----");

    let mut http_get_buf = [0u8; 256];
    let http_get = match hostname {
        Some(host_bytes) => {
            // Trim null/padding bytes from the fixed-size hostname array
            let host_len = host_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(host_bytes.len());
            let host_slice = &host_bytes[..host_len];
            let base = b"GET / HTTP/1.1\r\nHost: ";
            let suffix = b"\r\n\r\n";
            let mut pos = 0;

            http_get_buf[..base.len()].copy_from_slice(base);
            pos += base.len();

            http_get_buf[pos..pos + host_slice.len()].copy_from_slice(host_slice);
            pos += host_slice.len();

            http_get_buf[pos..pos + suffix.len()].copy_from_slice(suffix);
            pos += suffix.len();

            &http_get_buf[..pos]
        }
        None => b"GET / HTTP/1.1\r\n\r\n",
    };

    let nbytes = tcp_client.write(http_get).await?;

    info!("-----Request sent {}-----", nbytes);
    let mut respbuf = [0; 1500];
    let resplen = tcp_client.read(&mut respbuf).await?;
    info!("-----Response received {}-----", resplen);
    let the_received_slice = &respbuf[..resplen];
    let recvd_str = match core::str::from_utf8(the_received_slice) {
        Err(err) => core::str::from_utf8(&the_received_slice[..err.valid_up_to()])
            .unwrap_or("Invalid bytes received."),
        Ok(s) => s,
    };
    info!("-----Response: {}-----", recvd_str);

    Ok(())
}
