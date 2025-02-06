use super::info;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use embedded_nal::nb::block;
use embedded_nal::UdpClientStack;

pub fn coap_client<S>(stack: &mut S, addr: Ipv4Addr, port: u16) -> Result<(), S::Error>
where
    S::Error: core::fmt::Debug,
    S: UdpClientStack + ?Sized,
{
    let mut sock = stack.socket()?;
    let target = SocketAddr::new(IpAddr::V4(addr), port);
    stack.connect(&mut sock, target)?;
    // Data, V1 NON no token, GET, message ID 0x0000, 2x Uri-Path
    block!(stack.send(&mut sock, b"\x50\x01\0\0\xbb.well-known\x04core"))?;

    let mut respbuf = [0; 1500];
    let (resplen, _) = block!(stack.receive(&mut sock, &mut respbuf))?;
    let response = &respbuf[..resplen];

    info!("Response: {}", core::str::from_utf8(response).unwrap());

    Ok(())
}
