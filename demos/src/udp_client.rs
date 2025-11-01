use super::{error, info};
use embedded_nal::nb::block;
use embedded_nal::UdpClientStack;

use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use super::Ipv4AddrWrap;

pub fn udp_client<T, S>(stack: &mut T, addr: Ipv4Addr, port: u16) -> Result<(), T::Error>
where
    T: UdpClientStack<UdpSocket = S> + ?Sized,
    T::Error: core::fmt::Debug,
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
        stack.connect(&mut s, remote)?;
        info!("-----Socket connected-----");
        let http_get: &str = "UDP /v1\r\n\r\n";
        let nbytes = block!(stack.send(&mut s, http_get.as_bytes()))?;
        info!("-----Request sent {:?}-----", nbytes);
        let mut respbuf = [0; 1500];

        info!("-----Response entering block");
        let (resplen, addr) = block!(stack.receive(&mut s, &mut respbuf))?;
        info!("-----Response passed block");
        match addr {
            SocketAddr::V4(sa) => {
                info!(
                    "-----Response received from {}----- {:?}",
                    resplen,
                    Ipv4AddrWrap { addr: sa.ip() }
                );
            }
            SocketAddr::V6(_sa) => {
                unreachable!("Shouldn't get here")
            }
        }
        let the_received_slice = &respbuf[..resplen];
        let recvd_str = core::str::from_utf8(the_received_slice).unwrap();
        info!("-----Response: {}-----", recvd_str);
        stack.close(s)?;
    } else {
        error!("Socket creation failed");
    }
    Ok(())
}
