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

use crate::readwrite::BufferOverflow;
use crate::readwrite::Write;

use crate::socket::Socket;
use core::net::{Ipv4Addr, SocketAddrV4};

use super::constants::AuthType;

// todo: support other auth besides Open/WPA
pub fn write_connect_request(
    auth: AuthType,
    ssid: &str,
    password: &str,
    channel: u16,
    dont_save_creds: bool,
) -> Result<[u8; 108], BufferOverflow> {
    let mut result = [0xCCu8; 108];
    let mut slice = result.as_mut_slice();
    if password.len() > 63 {
        return Err(BufferOverflow);
    }
    slice.write(password.as_bytes())?;
    slice.write(&[0])?;
    let mut slice = &mut result[65..];
    slice.write(&[auth.into()])?;
    slice.write(&[0xCC, 0xCC])?;
    slice.write(&channel.to_le_bytes())?;
    slice.write(ssid.as_bytes())?;
    slice.write(&[0])?;
    result[103] = dont_save_creds as u8;
    Ok(result)
}

// tstrM2MScan
pub fn write_scan_req(channel: u8, scantime: u16) -> Result<[u8; 4], BufferOverflow> {
    let mut result = [0u8; 4];
    let mut slice = result.as_mut_slice();
    slice.write(&channel.to_le_bytes())?;
    slice.write(&[0u8])?; // reserved
    slice.write(&scantime.to_le_bytes())?;
    Ok(result)
}

// no request struct, just C-string
pub fn write_gethostbyname_req<'a, const N: usize>(
    host: &str,
    buffer: &'a mut [u8; N],
) -> Result<&'a [u8], BufferOverflow> {
    let len = host.len();
    if len + 1 > buffer.len() {
        return Err(BufferOverflow);
    }
    buffer[0..len].copy_from_slice(host.as_bytes());
    buffer[len] = 0;
    Ok(&buffer[0..len + 1])
}

// tstrPingCmd
pub fn write_ping_req(
    dest_ip: Ipv4Addr,
    ttl: u8,
    count: u16,
    marker: u8,
) -> Result<[u8; 12], BufferOverflow> {
    let mut result = [0x0u8; 12];
    let mut slice = result.as_mut_slice();
    let ip: u32 = dest_ip.into();
    slice.write(&(ip).to_be_bytes())?;
    slice.write(&[marker, 0xBE, 0xBE, 0xBE])?; // todo
    slice.write(&count.to_le_bytes())?;
    slice.write(&[ttl])?;
    Ok(result)
}

// tstrBindCmd
pub fn write_bind_req(
    socket: Socket,
    address_family: u16,
    address: SocketAddrV4,
) -> Result<[u8; 12], BufferOverflow> {
    let mut result = [0x0u8; 12];
    let mut slice = result.as_mut_slice();
    let ip: u32 = (*address.ip()).into();
    slice.write(&address_family.to_le_bytes())?;
    slice.write(&address.port().to_be_bytes())?;
    slice.write(&ip.to_be_bytes())?;
    slice.write(&[socket.v, 0])?;
    slice.write(&socket.s.to_le_bytes())?;
    Ok(result)
}

// tstrConnectCmd
pub fn write_connect_req(
    socket: Socket,
    address_family: u16,
    address: SocketAddrV4,
    ssl_flags: u8,
) -> Result<[u8; 12], BufferOverflow> {
    let mut result = [0x0u8; 12];
    let mut slice = result.as_mut_slice();
    let ip: u32 = (*address.ip()).into();
    slice.write(&address_family.to_le_bytes())?;
    slice.write(&address.port().to_be_bytes())?;
    slice.write(&ip.to_be_bytes())?;
    slice.write(&[socket.v, ssl_flags])?;
    slice.write(&socket.s.to_le_bytes())?;
    Ok(result)
}

// tstrSendCmd
pub fn write_sendto_req(
    socket: Socket,
    address_family: u16,
    address: SocketAddrV4,
    len: usize,
) -> Result<[u8; 16], BufferOverflow> {
    let mut result = [0x0u8; 16];
    let mut slice = result.as_mut_slice();
    let ip: u32 = (*address.ip()).into();
    slice.write(&[socket.v, 0])?;
    slice.write(&(len as u16).to_le_bytes())?;
    slice.write(&address_family.to_le_bytes())?;
    slice.write(&address.port().to_be_bytes())?;
    slice.write(&ip.to_be_bytes())?;
    slice.write(&socket.s.to_le_bytes())?;
    slice.write(&[0, 0])?;
    Ok(result)
}

// tstrListenCmd
pub fn write_listen_req(socket: Socket, backlog: u8) -> Result<[u8; 4], BufferOverflow> {
    let mut result = [0x0u8; 4];
    let mut slice = result.as_mut_slice();
    slice.write(&[socket.v, backlog])?;
    slice.write(&socket.s.to_le_bytes())?;
    Ok(result)
}

//tstrRecvCmd
pub fn write_recv_req(socket: Socket, timeout: u32) -> Result<[u8; 8], BufferOverflow> {
    let mut result = [0x0u8; 8];
    let mut slice = result.as_mut_slice();
    slice.write(&timeout.to_le_bytes())?;
    slice.write(&[socket.v, 0])?;
    slice.write(&socket.s.to_le_bytes())?;
    Ok(result)
}

// tstrCloseCmd
pub fn write_close_req(socket: Socket) -> Result<[u8; 4], BufferOverflow> {
    let mut result = [0x0u8; 4];
    let mut slice = result.as_mut_slice();
    slice.write(&[socket.v, 0])?;
    slice.write(&socket.s.to_le_bytes())?;
    Ok(result)
}

// tstrSetSocketOptCmd
pub fn write_setsockopt_req(
    socket: Socket,
    option: u8,
    value: u32,
) -> Result<[u8; 8], BufferOverflow> {
    let mut result = [0x0u8; 8];
    let mut slice = result.as_mut_slice();
    slice.write(&value.to_le_bytes())?;
    slice.write(&[socket.v, option])?;
    slice.write(&socket.s.to_le_bytes())?;
    Ok(result)
}

pub fn write_prng_req(data_addr: u32, data_size: u16) -> Result<[u8; 8], BufferOverflow> {
    let mut req = [0x00u8; 8];
    let mut slice = req.as_mut_slice();
    slice.write(&data_addr.to_le_bytes())?;
    slice.write(&data_size.to_le_bytes())?;
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_scan() {
        assert_eq!(write_scan_req(1, 12).unwrap(), [1, 0, 12, 0]);
        assert_eq!(write_scan_req(255, 258).unwrap(), [255, 0, 2, 1]);
    }
    #[test]
    fn test_ping() {
        assert_eq!(
            write_ping_req(Ipv4Addr::new(1, 2, 3, 4), 5, 258, 0xDE).unwrap(),
            [1, 2, 3, 4, 0xDE, 0xBE, 0xBE, 0xBE, 2, 1, 5, 0]
        );
        assert_eq!(
            write_ping_req(Ipv4Addr::new(255, 2, 3, 4), 4, 258, 0xBA).unwrap(),
            [0xFF, 2, 3, 4, 0xBA, 0xBE, 0xBE, 0xBE, 2, 1, 4, 0]
        );
    }

    #[test]
    fn test_dns() {
        let mut buff = [0u8; 6];
        assert_eq!(
            write_gethostbyname_req("abc", &mut buff).unwrap(),
            [97, 98, 99, 0]
        );
        assert_eq!(
            write_gethostbyname_req("abcde", &mut buff).unwrap(),
            [97, 98, 99, 100, 101, 0]
        );
        assert!(matches!(
            write_gethostbyname_req("abcdef", &mut buff),
            Err(BufferOverflow)
        ));
    }
    #[test]
    fn test_bind() {
        assert_eq!(
            write_bind_req(
                Socket::new(7, 521),
                2,
                SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 32769)
            )
            .unwrap(),
            [
                2, 0, // af
                128, 1, 1, 2, 3, 4, 7, 0, 9, 2
            ]
        );
        assert_eq!(
            write_bind_req(
                Socket::new(0, 3),
                257,
                SocketAddrV4::new(Ipv4Addr::new(255, 2, 3, 4), 1000)
            )
            .unwrap(),
            [1, 1, 3, 232, 0xFF, 2, 3, 4, 0, 0, 3, 0]
        )
    }
    #[test]
    fn test_connect() {
        assert_eq!(
            write_connect_req(
                Socket::new(7, 1030),
                2,
                SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 0xFABA),
                42
            )
            .unwrap(),
            [
                2, 0, // address family
                0xFA, 0xBA, // port
                1, 2, 3, 4, //ip
                7, 42, 6, 4
            ]
        );
        assert_eq!(
            write_connect_req(
                Socket::new(0, 1),
                2,
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 196), 20002),
                0
            )
            .unwrap(),
            [
                2, 0, // addr
                0x4E, 0x22, // port
                0xC0, 0xA8, 0x5, 0xC4, // ipaddr
                0x00, // sock
                0,    // sslflags
                0x1, 00 // session
            ]
        );
    }
    #[test]
    fn test_sendto() {
        assert_eq!(
            write_sendto_req(
                Socket::new(7, 521),
                2,
                SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 0xFABA),
                10
            )
            .unwrap(),
            [7, 0, 10, 0, 2, 0, 0xFA, 0xBA, 1, 2, 3, 4, 9, 2, 0, 0,]
        );
        assert_eq!(
            write_sendto_req(
                Socket::new(7, 521),
                2,
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 196), 0x214E),
                10
            )
            .unwrap(),
            [7, 0, 10, 0, 2, 0, 0x21, 0x4E, 192, 168, 5, 196, 9, 2, 0, 0,]
        )
    }

    #[test]
    fn test_listen() {
        assert_eq!(
            write_listen_req(Socket::new(1, 258), 2).unwrap(),
            [1, 2, 2, 1]
        );
    }
    #[test]
    fn test_recv() {
        assert_eq!(
            write_recv_req(Socket::new(1, 258), 0xDEADBEEF).unwrap(),
            [0xEF, 0xBE, 0xAD, 0xDE, 1, 0, 2, 1]
        )
    }

    #[test]
    fn test_close() {
        assert_eq!(write_close_req(Socket::new(1, 258)).unwrap(), [1, 0, 2, 1]);
    }
    #[test]
    fn test_setsockopt() {
        assert_eq!(
            write_setsockopt_req(Socket::new(1, 258), 42, 0xDEADBEEF).unwrap(),
            [0xEF, 0xBE, 0xAD, 0xDE, 1, 42, 2, 1]
        );
    }

    #[test]
    fn test_wpa_connect() {
        let mut test_vector = [
            0x73u8, 0x75, 0x70, 0x65, 0x72, 0x5F, 0x73, 0x65, 0x63, 0x72, 0x65, 0x74, 0x5F, 0x70,
            0x61, 0x73, 0x73, 0x77, 0x6F, 0x72, 0x64, 0x00, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0x02, 0xCC, 0xCC, 0x09, 0x00,
            0x73, 0x61, 0x6D, 0x70, 0x6C, 0x65, 0x5F, 0x73, 0x73, 0x69, 0x64, 0x00, 0xCC, 0xCC,
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
            0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0x01, 0xCC, 0xCC, 0xCC, 0xCC,
        ];
        let test_ssid = "sample_ssid";
        let test_pass = "super_secret_password";
        assert_eq!(
            test_vector,
            write_connect_request(AuthType::WpaPSK, test_ssid, test_pass, 9, true).unwrap()
        );
        test_vector[65] = 1;
        test_vector[68] = 2;
        test_vector[103] = 0;
        assert_eq!(
            test_vector,
            write_connect_request(AuthType::Open, test_ssid, test_pass, 2, false).unwrap()
        );
        assert!(matches!(
            write_connect_request(
                AuthType::Open,
                "how about them ssid be way overly verbose \
                without anyones approval at all",
                test_pass,
                2,
                false
            ),
            Err(BufferOverflow)
        ));
        assert!(matches!(
            write_connect_request(
                AuthType::Open,
                test_ssid,
                "this password may be way too long for anything \
                to fit into the buffer here",
                2,
                false
            ),
            Err(BufferOverflow)
        ));
    }
}
