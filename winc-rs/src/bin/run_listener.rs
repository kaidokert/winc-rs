// Copyright 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use wincwifi::error::Error;
use wincwifi::manager::{AuthType, EventListener, Manager};
use wincwifi::transfer::PrefixXfer;
use wincwifi::Socket;
use wincwifi::{debug, info};
use wincwifi::{Ipv4Addr, SocketAddrV4};

#[cfg(feature = "std")]
use genio::std_impls::GenioIo;
#[cfg(feature = "std")]
use std::net::TcpStream;

use simple_logger::init_with_env;

#[cfg(not(feature = "std"))]
fn main() -> Result<(), Error> {
    Ok(())
}

pub struct Callbacks;
impl EventListener for Callbacks {
    fn on_rssi(&mut self, rssi: i8) {
        info!("Got RSSI {:?}", rssi)
    }
    fn on_resolve(&mut self, ip: Ipv4Addr, host: &str) {
        info!("OVER: Got DNS resolve, ip: {} host: {}", ip, host)
    }
}

#[cfg(feature = "std")]
fn main() -> Result<(), Error> {
    init_with_env().map_err(|_| Error::Failed)?;

    debug!("Starting");
    let prints = format!("{}:{}", "localhost", 9030);
    let stream_ = TcpStream::connect(prints).unwrap();

    type StreamType<'a> = GenioIo<&'a TcpStream>;
    let mut stream: StreamType = GenioIo::new(&stream_);

    let mut manager = Manager::from_xfer(PrefixXfer::new(&mut stream), Callbacks {});

    manager.set_crc_state(true);
    //    manager.send_scan(3, 257)?;
    debug!("Chip id: {:x}", manager.chip_id()?);
    debug!("Chip rev: {:x}", manager.chip_rev()?);

    //manager.get_systime();

    manager.send_default_connect()?;
    manager.dispatch_events()?;
    manager.dispatch_events()?;
    manager.dispatch_events()?;

    manager.send_connect(AuthType::Open, "ssid", "password", 42, false)?;
    manager.dispatch_events()?;

    manager.send_get_current_rssi()?;
    manager.dispatch_events()?;

    manager.send_get_conn_info()?;
    manager.dispatch_events()?;

    manager.send_scan(0xFF, 513)?;
    manager.dispatch_events()?;
    manager.dispatch_events()?;
    manager.dispatch_events()?;
    manager.dispatch_events()?;

    manager.send_get_scan_result(1)?;
    manager.dispatch_events()?;
    manager.send_ping_req(Ipv4Addr::new(192, 168, 5, 196), 10, 4, 0x4A)?;
    manager.dispatch_events()?;
    manager.dispatch_events()?;

    manager.send_gethostbyname("google.com")?;
    manager.dispatch_events()?;

    manager.send_bind(
        Socket::new(42, 512 + 10),
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 101), 3000),
    )?;
    manager.dispatch_events()?;

    manager.send_listen(Socket::new(42, 522), 4)?;
    manager.dispatch_events()?;
    manager.dispatch_events()?;

    manager.send_socket_connect(
        Socket::new(0, 1),
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 196), 20002),
    )?;
    manager.dispatch_events()?;

    let thestr = "lel";
    manager.send_sendto(
        Socket::new(7, 1),
        SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 196), 20001),
        &[65, 66, 67],
    )?;
    manager.dispatch_events()?;

    manager.send_send(Socket::new(7, 12), thestr.as_bytes())?;
    manager.dispatch_events()?;

    manager.send_recv(Socket::new(7, 12), 1000)?;
    manager.dispatch_events()?;

    // send_recvfrom
    manager.send_recvfrom(Socket::new(7, 12), 1000)?;
    manager.dispatch_events()?;

    manager.send_setsockopt(Socket::new(3, 513), 2, 65538)?;
    manager.send_close(Socket::new(3, 513))?;

    Ok(())
}

// make this into defmt threaded receiver

#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn flush() {}
    unsafe fn release() {}
    unsafe fn write(_bytes: &[u8]) {}
}

defmt::timestamp!("{=u32}", 0);

#[defmt::panic_handler]
fn panic() -> ! {
    core::panic!("panic via `defmt::panic!`")
}
