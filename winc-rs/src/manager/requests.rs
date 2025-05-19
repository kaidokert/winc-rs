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

use crate::manager::net_types::WpaKey;
use crate::readwrite::BufferOverflow;
use crate::readwrite::Write;

use crate::socket::Socket;
use core::net::{Ipv4Addr, SocketAddrV4};

use super::constants::{AuthType, MAX_PSK_KEY_LEN, START_PROVISION_PACKET_SIZE};

use super::net_types::WepKey;
use super::{AccessPoint, Credentials, HostName};

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
    if password.len() > MAX_PSK_KEY_LEN {
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

/// Prepares the packet for a PRNG request.
///
/// Packet Structure:
///
/// | Input Buffer Address | Number of Random Bytes to Generate | Padding |
/// |----------------------|------------------------------------|---------|
/// | 4 Bytes              | 2 Bytes                            | 2 Bytes |
///
/// # Arguments
///
/// * `addr` - The address of the input buffer for storing PRNG data.
/// * `len` - The length of the input buffer, or the number of random bytes to generate.
///
/// # Returns
///
/// * `[u8]` - An array of 8 bytes representing the request packet for PRNG.
/// * `BufferOverflow` - If the data exceeds the buffer limit during packet preparation.
pub fn write_prng_req(addr: u32, len: u16) -> Result<[u8; 8], BufferOverflow> {
    let mut req = [0x00u8; 8];
    let mut slice = req.as_mut_slice();
    slice.write(&addr.to_le_bytes())?;
    slice.write(&len.to_le_bytes())?;
    Ok(req)
}

/// Prepares the packet to start the provisioning mode.
///
/// Packet Structure:
///
/// |       Name        | Bytes |      Name        | Bytes |
/// |-------------------|-------|------------------|-------|
/// |       SSID        |   33  |     Channel      |   1   |
/// |     Key Size      |   1   |     Wep key      |   1   |
/// |     Wep key       |   27  |     Security     |   1   |
/// |  SSID Visibility  |   1   |  DHCP server IP  |   4   |
/// |      WPA Key      |   65  |     Padding      |   2   |
/// |      DNS url      |   64  |   Http Redirect  |   1   |
/// |      Padding      |   3   |                  |       |
///
///
/// # Arguments
///
/// * `ap` - An `AccessPoint` struct containing the SSID, passphrase, and other network details.
/// * `dns` - DNS redirect URL as a string slice (max 63 bytes).
/// * `http_redirect` - Whether HTTP redirect is enabled.
///
/// # Returns
///
/// * `[u8; START_PROVISION_PACKET_SIZE])` - The provisioning request packet as a fixed-size byte array.
/// * `BufferOverflow` - If the input data exceeds allowed size or the buffer limit.
pub fn write_start_provisioning_req(
    ap: &AccessPoint,
    hostname: &HostName,
    http_redirect: bool,
) -> Result<[u8; START_PROVISION_PACKET_SIZE], BufferOverflow> {
    let mut req = [0u8; START_PROVISION_PACKET_SIZE];
    let mut slice = req.as_mut_slice();

    // Set parameters for WEP
    let wep_key_index: u8;
    let wep_key: WepKey;
    #[cfg(feature = "wep")]
    {
        if let Credentials::Wep(key, index) = ap.key {
            wep_key_index = index.into();
            wep_key = key;
        } else {
            wep_key_index = 0;
            wep_key = WepKey::new();
        }
    }

    #[cfg(not(feature = "wep"))]
    {
        wep_key_index = 0;
        wep_key = WepKey::new();
    }

    // Set parameters for WPA-PSK
    let wpa_key = if let Credentials::WpaPSK(key) = ap.key {
        key
    } else {
        WpaKey::new()
    };

    // dhcp
    let dhcp: u32 = ap.ip.into();

    // SSID
    slice.write(ap.ssid.as_bytes())?;
    // Null termination
    slice = &mut req[32..];
    slice.write(&[0u8])?;
    // WiFi channel
    slice.write(&[(ap.channel).into()])?;
    // Wep key Index
    slice.write(&[wep_key_index])?;
    // Wep/WPA key size
    slice.write(&[ap.key.key_len() as u8])?;
    // Wep key
    slice.write(wep_key.as_bytes())?;
    // Null termination
    slice = &mut req[62..];
    slice.write(&[0u8])?;
    // Security type
    slice.write(&[(ap.key).into()])?;
    // SSID visibility
    slice.write(&[ap.ssid_hidden as u8])?;
    // dhcp server
    slice.write(&dhcp.to_be_bytes())?;
    // WPA key
    slice.write(wpa_key.as_bytes())?;
    // WINC firmware supports 64 bytes (+1 over standard) plus null terminator.
    slice = &mut req[132..];
    // Null termination
    slice.write(&[0u8, 0u8])?;
    // Padding
    slice.write(&[0u8, 0u8])?;
    // Device Domain name
    slice.write(hostname.as_bytes())?;
    // Null termination
    slice = &mut req[199..];
    slice.write(&[0u8])?;
    // Http redirect
    slice.write(&[http_redirect as u8])?;
    // Padding
    slice.write(&[0u8, 0u8, 0u8])?;

    Ok(req)
}

#[cfg(test)]
mod tests {
    use crate::Ssid;

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

    #[test]
    fn test_prng_request() {
        let request = [0xDC, 0x65, 0x00, 0x20, 0x20, 0x00, 0x00, 0x00];
        let addr = 0x200065DC;
        let len = 32;
        assert_eq!(write_prng_req(addr, len).unwrap(), request);
    }

    #[test]
    fn test_start_provisioning_request() {
        let valid_req: [u8; START_PROVISION_PACKET_SIZE] = [
            /* Ssid */ 116, 101, 115, 116, 95, 115, 115, 105, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, /* Wifi Channel */ 1,
            /* Wep key Index */ 0, /* Wep/Wpa Key Size */ 13, /* Wep key */ 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* Security Type */ 2, /* ssid hidden */ 0, /* DHCP Server */ 0xC0,
            0xA8, 0x01, 0x01, /* WPA Key */ 116, 101, 115, 116, 95, 112, 97, 115, 115, 119,
            111, 114, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* padding */ 0, 0, /* hostname */ 97, 100, 109, 105, 110, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            /* Http redirect */ 0, /* padding */ 0, 0, 0,
        ];
        let ap_ssid = Ssid::from("test_ssid").unwrap();
        let psk = WpaKey::from("test_password").unwrap();
        let access_point = AccessPoint::wpa(&ap_ssid, &psk);
        let hostname = HostName::from("admin").unwrap();

        let result = write_start_provisioning_req(&access_point, &hostname, false).unwrap();

        assert_eq!(result, valid_req);
    }
}
