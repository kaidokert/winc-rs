use super::{error, info};
use embedded_nal::nb::block;
use embedded_nal::TcpClientStack;

use core::net::{IpAddr, Ipv4Addr, SocketAddr};

pub fn http_client<T, S>(
    stack: &mut T,
    addr: Ipv4Addr,
    port: u16,
    hostname: Option<&str>,
) -> Result<(), T::Error>
where
    T: TcpClientStack<TcpSocket = S> + ?Sized,
    T::Error: embedded_nal::TcpError,
{
    let sock = stack.socket();
    if let Ok(mut s) = sock {
        info!(
            "-----connecting to ----- {}.{}.{}.{} port {}",
            addr.octets()[0],
            addr.octets()[1],
            addr.octets()[2],
            addr.octets()[3],
            port
        );
        let remote = SocketAddr::new(IpAddr::V4(addr), port);
        block!(stack.connect(&mut s, remote))?;
        info!("-----Socket connected-----");
        let mut http_get_buf = [0u8; 256];
        let http_get = match hostname {
            Some(host) => {
                let base = b"GET / HTTP/1.1\r\nHost: ";
                let suffix = b"\r\n\r\n";
                let mut pos = 0;

                http_get_buf[..base.len()].copy_from_slice(base);
                pos += base.len();

                let host_bytes = host.as_bytes();
                http_get_buf[pos..pos + host_bytes.len()].copy_from_slice(host_bytes);
                pos += host_bytes.len();

                http_get_buf[pos..pos + suffix.len()].copy_from_slice(suffix);
                pos += suffix.len();

                &http_get_buf[..pos]
            }
            None => b"GET / HTTP/1.1\r\n\r\n",
        };
        let nbytes = stack.send(&mut s, http_get);
        info!("-----Request sent {}-----", nbytes.unwrap());
        let mut respbuf = [0; 1500];
        let resplen = block!(stack.receive(&mut s, &mut respbuf))?;
        info!("-----Response received {}-----", resplen);
        let the_received_slice = &respbuf[..resplen];
        let recvd_str = core::str::from_utf8(the_received_slice).unwrap();
        info!("-----Response: {}-----", recvd_str);
        stack.close(s)?;
    } else {
        error!("Socket creation failed");
    }
    Ok(())
}
