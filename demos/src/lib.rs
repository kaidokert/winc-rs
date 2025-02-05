// make this no_std
#![no_std]

#[cfg(not(feature = "std"))]
use defmt::{debug, error, info};
#[cfg(feature = "std")]
use log::{debug, error, info};

pub mod http_client;
pub mod tcp_server;
pub mod udp_server;

#[derive(Debug)]
struct Ipv4AddrWrap<'a> {
    addr: &'a core::net::Ipv4Addr,
}

#[derive(Debug)]
struct IpAddrWrap<'a> {
    addr: &'a core::net::IpAddr,
}

#[derive(Debug)]
struct SocketAddrWrap<'a> {
    addr: &'a core::net::SocketAddr,
}

#[derive(Debug)]
struct SocketAddrV4Wrap<'a> {
    addr: &'a core::net::SocketAddrV4,
}

#[cfg(feature = "defmt")]
impl defmt::Format for Ipv4AddrWrap<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        let ip: u32 = (*self.addr).into();
        defmt::write!(
            fmt,
            "{=u8}.{=u8}.{=u8}.{=u8}",
            ((ip >> 24) & 0xFF) as u8,
            ((ip >> 16) & 0xFF) as u8,
            ((ip >> 8) & 0xFF) as u8,
            ((ip >> 0) & 0xFF) as u8,
        );
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for IpAddrWrap<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        match self.addr {
            core::net::IpAddr::V4(addr) => defmt::write!(fmt, "{}", Ipv4AddrWrap { addr }),
            _ => panic!("Unsupported IP address type"),
        }
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for SocketAddrV4Wrap<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "{}:{}",
            Ipv4AddrWrap {
                addr: self.addr.ip()
            },
            self.addr.port()
        );
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for SocketAddrWrap<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        match self.addr {
            core::net::SocketAddr::V4(addr) => defmt::write!(fmt, "{}", SocketAddrV4Wrap { addr }),
            _ => panic!("unsupported"),
        }
    }
}
