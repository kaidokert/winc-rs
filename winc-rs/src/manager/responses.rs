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

use crate::readwrite::{Read, ReadExactError};
use core::{
    net::{Ipv4Addr, SocketAddrV4},
    str::FromStr,
};

use super::constants::{
    AuthType, PingError, SocketError, AF_INET, CONN_INFO_RESP_PACKET_SIZE, MAX_HOST_NAME_LEN,
    MAX_PSK_KEY_LEN, MAX_SSID_LEN, SCAN_RESULT_RESP_PACKET_SIZE,
};
use crate::errors::CommError as Error;

use super::net_types::{Bssid, HostName, MacAddress, Ssid, WpaKey};
#[cfg(feature = "experimental-ecc")]
use super::{
    constants::EccRequestType,
    net_types::{EccRequest, EcdhInfo, EcdsaSignInfo},
};
use arrayvec::ArrayString;

use crate::{error, socket::Socket, HexWrap};

#[cfg(feature = "defmt")]
use crate::Ipv4AddrFormatWrapper;

/// Length of the firmware version.
const FW_INFO_VER_LEN: usize = 6;
/// Length of the firmware build date.
const FW_INFO_BUILD_DATE_LEN: usize = 12;
/// Length of the firmware build time.
const FW_INFO_BUILD_TIME_LEN: usize = 9;
/// Total size of the firmware info response packet, in bytes.
const FW_INFO_RESP_PACKET_SIZE: usize = 40;
/// Error type returned when an reading from a byte slice fails.
type ErrType<'a> = ReadExactError<<&'a [u8] as Read>::ReadError>;

fn read32be<'a>(v: &mut &[u8]) -> Result<u32, ErrType<'a>> {
    let mut arr = [0u8; 4];
    v.read_exact(&mut arr)?;
    Ok(u32::from_be_bytes(arr))
}

fn read32le<'a>(v: &mut &[u8]) -> Result<u32, ErrType<'a>> {
    let mut arr = [0u8; 4];
    v.read_exact(&mut arr)?;
    Ok(u32::from_le_bytes(arr))
}

fn read16<'a>(v: &mut &[u8]) -> Result<u16, ErrType<'a>> {
    let mut arr = [0u8; 2];
    v.read_exact(&mut arr)?;
    Ok(u16::from_le_bytes(arr))
}

fn read16be<'a>(v: &mut &[u8]) -> Result<u16, ErrType<'a>> {
    let mut arr = [0u8; 2];
    v.read_exact(&mut arr)?;
    Ok(u16::from_be_bytes(arr))
}

fn read8<'a>(v: &mut &[u8]) -> Result<u8, ErrType<'a>> {
    let mut arr = [0u8; 1];
    v.read_exact(&mut arr)?;
    Ok(arr[0])
}

fn from_c_byte_str<const N: usize>(input: [u8; N]) -> Result<ArrayString<N>, core::str::Utf8Error> {
    let mut ret = ArrayString::from_byte_string(&input)?;
    if let Some(i) = &ret.find('\0') {
        ret.truncate(*i);
    }
    Ok(ret)
}

fn from_c_byte_slice<const N: usize>(input: &[u8]) -> Result<ArrayString<N>, Error> {
    let slice = match core::str::from_utf8(input) {
        Err(err) => core::str::from_utf8(&input[..err.valid_up_to()])?,
        Ok(s) => s,
    };
    let mut ret = ArrayString::<N>::from_str(slice)?;
    if let Some(i) = &ret.find('\0') {
        ret.truncate(*i);
    }
    Ok(ret)
}

/// A revision number of the firmware
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq)]
pub struct Revision {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

/// Information about the firmware version of the Wifi module
pub struct FirmwareInfo {
    pub chip_id: u32,
    pub firmware_revision: Revision,
    pub driver_revision: Revision,
    pub build_date: ArrayString<FW_INFO_BUILD_DATE_LEN>,
    pub build_time: ArrayString<FW_INFO_BUILD_TIME_LEN>,
    pub svn_rev: u16,
}

impl From<[u8; FW_INFO_RESP_PACKET_SIZE]> for FirmwareInfo {
    fn from(data: [u8; FW_INFO_RESP_PACKET_SIZE]) -> Self {
        let mut data_slice = data.as_slice();
        let reader = &mut data_slice;

        let mut ver = [0u8; FW_INFO_VER_LEN];
        let mut build_date = [0u8; FW_INFO_BUILD_DATE_LEN];
        let mut build_time = [0u8; FW_INFO_BUILD_TIME_LEN];

        // todo: get rid of unwraps
        let chip_id = read32le(reader).unwrap();
        reader.read_exact(&mut ver).unwrap();
        reader.read_exact(&mut build_date).unwrap();
        reader.read_exact(&mut build_time).unwrap();
        let _ = read8(reader).unwrap();
        let svn_rev = read16(reader).unwrap();

        FirmwareInfo {
            chip_id,
            firmware_revision: Revision {
                major: ver[0],
                minor: ver[1],
                patch: ver[2],
            },
            driver_revision: Revision {
                major: ver[3],
                minor: ver[4],
                patch: ver[5],
            },
            build_date: from_c_byte_str(build_date).unwrap(),
            build_time: from_c_byte_str(build_time).unwrap(),
            svn_rev,
        }
    }
}

/// Formatter for the firmware version.
#[cfg(feature = "defmt")]
impl defmt::Format for FirmwareInfo {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "Firmware rev: {}.{}.{}\n\
             Driver rev: {}.{}.{}\n\
             Build Date: {}\n\
             Build Time: {}\n\
             SVN rev: {}",
            self.firmware_revision.major,
            self.firmware_revision.minor,
            self.firmware_revision.patch,
            self.driver_revision.major,
            self.driver_revision.minor,
            self.driver_revision.patch,
            self.build_date.as_str(),
            self.build_time.as_str(),
            self.svn_rev
        );
    }
}

/// Connected network information
//#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq)]
pub struct ConnectionInfo {
    pub ssid: Ssid,
    pub auth: AuthType,
    pub ip: Ipv4Addr,
    pub mac: MacAddress,
    pub rssi: i8,
}

#[cfg(feature = "defmt")]
impl defmt::Format for ConnectionInfo {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "Connection Info:\n\
             ssid: {}\n\
             authtype: {:?}\n\
             ip: {}\n\
             mac: {:?}\n\
             rssi: {}",
            self.ssid.as_str(),
            self.auth,
            Ipv4AddrFormatWrapper::new(&self.ip),
            self.mac,
            self.rssi
        );
    }
}

impl From<[u8; CONN_INFO_RESP_PACKET_SIZE]> for ConnectionInfo {
    fn from(v: [u8; CONN_INFO_RESP_PACKET_SIZE]) -> Self {
        let mut ipslice = &v[34..38];
        let res = Self {
            auth: v[33].into(),
            rssi: v[44] as i8,
            ip: read32be(&mut ipslice).unwrap().into(),
            ssid: from_c_byte_slice(&v[..MAX_SSID_LEN]).unwrap(),
            mac: MacAddress::from_bytes(&v[38..44]).unwrap(),
        };
        res
    }
}

impl core::fmt::Display for ConnectionInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ssid:{} authtype:{:?} ip:{} mac:{:?} rssi:{}",
            self.ssid.as_str(),
            self.auth,
            self.ip,
            self.mac,
            self.rssi
        )
    }
}

/// Result of a scan for access points
#[derive(Debug, PartialEq, Default)]
pub struct ScanResult {
    pub index: u8,
    pub rssi: i8,
    pub auth: AuthType,
    pub channel: u8,
    pub bssid: Bssid,
    /// SSID of the access point
    pub ssid: Ssid,
}

// todo: Scanresult parsing is ugly
impl From<[u8; SCAN_RESULT_RESP_PACKET_SIZE]> for ScanResult {
    fn from(v: [u8; SCAN_RESULT_RESP_PACKET_SIZE]) -> Self {
        let mut res = Self {
            index: v[0],
            rssi: v[1] as i8,
            auth: v[2].into(),
            channel: v[3],
            ..Default::default()
        };
        res.bssid = from_c_byte_slice(&v[4..9]).unwrap();
        res.ssid = from_c_byte_slice(&v[10..42]).unwrap();
        res
    }
}

impl core::fmt::Display for ScanResult {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "index:{} rssi:{} authtype:{:?} channel:{} bssid:{:?} ssid:{}",
            self.index,
            self.rssi,
            self.auth,
            self.channel,
            self.bssid,
            self.ssid.as_str()
        )
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for ScanResult {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "index:{} rssi:{} authtype:{:?} channel:{} bssid:{:?} ssid:{}",
            self.index,
            self.rssi,
            self.auth,
            self.channel,
            self.bssid.as_str(),
            self.ssid.as_str()
        );
    }
}

// tstrPingReply
pub fn read_ping_reply<'a>(
    mut response: &[u8],
) -> Result<(Ipv4Addr, u32, u32, u16, u16, PingError), ErrType<'a>> {
    let reader = &mut response;
    let ip = read32be(reader)?;
    let privx = read32be(reader)?;
    let rtt = read32le(reader)?;
    let succ = read16(reader)?;
    let fail = read16(reader)?;
    let errcode = read8(reader)?;
    let _ = read8(reader)?;
    let _ = read16(reader)?;
    Ok((ip.into(), privx, rtt, succ, fail, errcode.into()))
}

// tstrAcceptReply
pub fn read_accept_reply(
    mut response: &[u8],
) -> Result<(SocketAddrV4, Socket, Socket, u16), Error> {
    let reader = &mut response;
    if read16(reader)? != AF_INET {
        error!("Error response: {:x}", HexWrap { v: response });
        return Err(Error::UnexpectedAddressFamily);
    }
    let port = read16be(reader)?;
    let ip = read32be(reader)?;
    Ok((
        SocketAddrV4::new(ip.into(), port),
        (read8(reader)?, 0).into(),
        (read8(reader)?, 0).into(),
        read16(reader)?,
    ))
}

#[derive(PartialEq, Debug, Clone)]
pub struct IPConf {
    pub ip: Ipv4Addr,
    pub gateway: Ipv4Addr,
    pub dns: Ipv4Addr,
    pub subnet: Ipv4Addr,
    pub lease_time: u32,
}

impl core::fmt::Display for IPConf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ip:{} gateway:{} dns:{} subnet:{} lease:{}",
            self.ip, self.gateway, self.dns, self.subnet, self.lease_time
        )
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for IPConf {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "lease_time:{} ip:{} gateway:{} dns:{} subnet:{}",
            self.lease_time,
            Ipv4AddrFormatWrapper::new(&self.ip),
            Ipv4AddrFormatWrapper::new(&self.gateway),
            Ipv4AddrFormatWrapper::new(&self.dns),
            Ipv4AddrFormatWrapper::new(&self.subnet),
        );
    }
}

pub fn read_dhcp_conf<'a>(mut response: &[u8]) -> Result<IPConf, ErrType<'a>> {
    let reader = &mut response;
    Ok(IPConf {
        ip: read32be(reader)?.into(),
        gateway: read32be(reader)?.into(),
        dns: read32be(reader)?.into(),
        subnet: read32be(reader)?.into(),
        lease_time: read32le(reader)?,
    })
}

// tstrDnsReply: returns hostname, IP
pub fn read_dns_reply(mut response: &[u8]) -> Result<(Ipv4Addr, HostName), Error> {
    let reader = &mut response;
    let mut strbuffer = [0u8; MAX_HOST_NAME_LEN];
    // read hostname
    reader.read_exact(&mut strbuffer)?;
    // read null terminator
    read8(reader)?;
    // read ip address
    let ip = read32be(reader)?;
    Ok((ip.into(), from_c_byte_str(strbuffer)?))
}

// tstrSendReply: returns socket, byes sent, session
pub fn read_send_reply<'a>(mut response: &[u8]) -> Result<(Socket, i16), ErrType<'a>> {
    let reader = &mut response;
    let sock = read8(reader)?; // sock
    _ = read8(reader)?; // void
    let bytes_sent = read16(reader)? as i16; // sentbytes
    let session = read16(reader)?; //session
    _ = read16(reader)?; // void
    Ok(((sock, session).into(), bytes_sent))
}

// tstrRecvReply
pub fn read_recv_reply(mut response: &[u8]) -> Result<(Socket, SocketAddrV4, i16, u16), Error> {
    let reader = &mut response;
    if read16(reader)? != AF_INET {
        error!("Error response: {:x}", HexWrap::new(response));
        return Err(Error::UnexpectedAddressFamily);
    }
    let port = read16be(reader)?;
    let ip = read32be(reader)?;
    let status = read16(reader)? as i16;
    let offset = read16(reader)?;
    let socket = read8(reader)?;
    let _ = read8(reader)?;
    let session = read16(reader)?;
    Ok((
        (socket, session).into(),
        SocketAddrV4::new(ip.into(), port),
        status,
        offset,
    ))
}

// tstrBindReply, tstrListenReply
pub fn read_common_socket_reply<'a>(
    mut response: &[u8],
) -> Result<(Socket, SocketError), ErrType<'a>> {
    let reader = &mut response;
    let socket = read8(reader)?;
    let err = read8(reader)?;
    let session = read16(reader)?;
    Ok(((socket, session).into(), err.into()))
}

/// Reads the connect socket command reply.
///
/// # Arguments
///
/// * `response` - Data received from the chip.
///
/// # Returns
///
/// - `Ok((socket, socket_error))`:
///     - `Socket` — The socket information returned by the module.
///     - `SocketError` — Any error that occurred while connecting to the server.
/// - `Err(Error)` — If an error occurred while reading the connect socket response.
pub(crate) fn read_connect_socket_reply(
    mut response: &[u8],
) -> Result<(Socket, SocketError), Error> {
    let reader = &mut response;

    // read socket (1 byte)
    let sock = read8(reader)?;
    // read error (1 byte)
    let err = read8(reader)?;
    // read data offset (2 byte)
    let _offset = read16(reader)?;

    let mut _socket = Socket::new(sock, 0);

    #[cfg(feature = "ssl")]
    _socket.set_ssl_data_offset(_offset);

    Ok((_socket, err.into()))
}

/// Reads the PRNG data packet from the response received from the chip.
///
/// Response Structure:
///
/// | Input Buffer Address | Number of Random Bytes Generated | Padding |
/// |----------------------|----------------------------------|---------|
/// | 4 Bytes              | 2 Bytes                          | 2 Bytes |
///
/// # Arguments
///
/// * `response` - Data received from the chip.
///
/// # Returns
///
/// * `u32` - The memory address of the input buffer.
/// * `u16` - The length of the generated random bytes.
/// * `Error` - If an error occurred while reading the PRNG response.
pub fn read_prng_reply(mut response: &[u8]) -> Result<(u32, u16), Error> {
    let reader = &mut response;

    let addr = read32le(reader)?; // memory address
    let len = read16(reader)?; // random bytes length
    let _ = read16(reader)?; // void
    Ok((addr, len))
}

/// Reads the provisioning information from the data packet received from the chip.
///
/// Response Structure:
///
/// |    SSID    | Passphrase | Security type | Provisioning status |
/// |------------|------------|---------------|---------------------|
/// |  33 Bytes  |  65 Bytes  |    1 Bytes    |       1 Byte        |
///
/// # Arguments
///
/// * `response` - Data received from the chip that contains provisioning information.
///
/// # Returns
///
/// * `(Ssid, Passphrase, u8, bool)` on success, containing:
///   - `Ssid`: The Wi-Fi SSID in bytes.
///   - `Passphrase`: The Wi-Fi passphrase in bytes.
///   - `u8`: The security type
///   - `bool`: The provisioning status (`true` if provisioned)
/// * `Err(Error)` if the data is invalid or incomplete.
pub fn read_provisioning_reply(mut response: &[u8]) -> Result<(Ssid, WpaKey, u8, bool), Error> {
    let reader = &mut response;
    let mut ssid = [0u8; MAX_SSID_LEN];
    let mut key = [0u8; MAX_PSK_KEY_LEN];
    // read the ssid
    reader.read_exact(&mut ssid)?;
    // read the null terminator
    read8(reader)?;
    // read the passphrase
    reader.read_exact(&mut key)?;
    // read the null terminator (+1 for extra PSK key byte)
    read16(reader)?;
    // read the security type
    let security_type = read8(reader)?;
    // read the provisioning status
    let provisioning_status: bool = (read8(reader)?) == 0;
    Ok((
        from_c_byte_str(ssid)?,
        from_c_byte_str(key)?,
        security_type,
        provisioning_status,
    ))
}

/// Reads the ECC request from the WINC module.
///
/// # Arguments
///
/// * `response` - Data received from the chip that contains an SSL ECC request.
///
/// # Returns
///
/// * `EccRequest` ECC request structure.
/// * `Err(Error)` if the data is invalid or incomplete.
#[cfg(feature = "experimental-ecc")]
pub fn read_ssl_ecc_response(mut response: &[u8]) -> Result<EccRequest, Error> {
    let reader = &mut response;
    let mut ecc_req = EccRequest::default();

    // ecc request type
    ecc_req.ecc_info.req = read16(reader)?.into();
    // status
    ecc_req.ecc_info.status = read16(reader)?;
    // user data
    ecc_req.ecc_info.user_data = read32le(reader)?;
    // sequence number
    ecc_req.ecc_info.seq_num = read32le(reader)?;

    match ecc_req.ecc_info.req {
        EccRequestType::ClientEcdh | EccRequestType::ServerEcdh => {
            let mut ecdh_info = EcdhInfo::default();

            // x-coordinate
            reader.read_exact(&mut ecdh_info.ecc_point.x_pos)?;
            // y-coordinate
            reader.read_exact(&mut ecdh_info.ecc_point.y_pos)?;
            // point size
            ecdh_info.ecc_point.point_size = read16(reader)?;
            // private key id
            ecdh_info.ecc_point.private_key_id = read16(reader)?;
            // secret / private key
            reader.read_exact(&mut ecdh_info.private_key)?;

            ecc_req.ecdh_info = Some(ecdh_info);
        }
        EccRequestType::GenerateSignature => {
            let ecdsa_sign = EcdsaSignInfo {
                curve_type: read16(reader)?.into(),
                hash_size: read16(reader)?,
            };

            ecc_req.ecdsa_sign_info = Some(ecdsa_sign);
        }
        EccRequestType::VerifySignature => {
            let ecdsa_verify = read32le(reader)?;
            ecc_req.ecdsa_verify_info = Some(ecdsa_verify);
        }

        EccRequestType::Unknown => {
            return Err(Error::InvalidHifResponse("Invalid ECC request received."));
        }
        _ => {
            // do nothing
        }
    }

    Ok(ecc_req)
}

/// Reads the Ethernet RX packet information from a data packet received from the chip.
///
/// # Arguments
///
/// * `response` - Buffer containing Ethernet RX packet information.
///
/// # Returns
///
/// * `Ok((size, offset))`:
///     - `size` (`u16`) - Number of bytes available to read.
///     - `offset` (`u16`) - Offset within the data where the packet begins.
/// * `Err(Error)` - if the data is invalid, incomplete, or cannot be parsed.
#[cfg(feature = "ethernet")]
pub fn read_eth_rx_reply(mut response: &[u8]) -> Result<(u16, u16), Error> {
    let reader = &mut response;

    let packet_size = read16(reader)?;
    let offset = read16(reader)?;

    Ok((packet_size, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "experimental-ecc")]
    use crate::manager::constants::EccCurveType;

    #[test]
    fn test_socket() {
        let s: Socket = (4, 1500).into();
        let v: (u8, u16) = s.into();
        assert_eq!(v.0, 4);
        assert_eq!(v.1, 1500);
    }

    #[test]
    fn parse_scan_result() {
        let result = [
            1, 2, 3, 4, 61, 61, 61, 61, 61, 0, 65, 66, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62,
            62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 67, 0, 0,
        ]; // 1 padding byte at the end
        let res: ScanResult = result.into();
        assert_eq!(res.index, 1);
        assert_eq!(res.rssi, 2);
        assert_eq!(res.auth, AuthType::WEP);
        assert_eq!(res.bssid.as_bytes(), [61, 61, 61, 61, 61]);
        assert_eq!(res.ssid.as_str(), "AB>>>>>>>>>>>>>>>>>>>>>>>>>>>>>C");
    }

    #[test]
    fn test_firmware_info() {
        let test_vector = [
            0x01u8, 0x01, 0x01, 0x01, 0x02, 0x03, 0x04, 0x05, 0x20, 0x07, 0x48, 0x5A, 0x5A, 0x5A,
            0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x47, 0x00, 0x42, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A,
            0x5A, 0x54, 0x00, 0x5A, 0xE9, 0x03, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A,
        ];
        let res: FirmwareInfo = test_vector.into();
        assert_eq!(res.chip_id, 0x01010101);
        assert_eq!(
            res.firmware_revision,
            Revision {
                major: 2,
                minor: 3,
                patch: 4
            }
        );
        assert_eq!(
            res.driver_revision,
            Revision {
                major: 5,
                minor: 32,
                patch: 7
            }
        );
        assert_eq!(&res.build_date, "HZZZZZZZZZG");
        assert_eq!(&res.build_time, "BZZZZZZT");
        assert_eq!(res.svn_rev, 1001);
    }

    #[test]
    fn parse_connection_info() {
        let src = [
            65, 66, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 0x78, 62, 62, 62, 62, 62,
            62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 67, 0, 0x04, // auth
            0x01, 0x02, 0x03, 0x04, //ip
            0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, // mac
            0xAB, // rssi
            0xCC, 0xCC, 0xCC,
        ];
        let res: ConnectionInfo = src.into();
        assert_eq!(res.ssid.as_str(), "AB>>>>>>>>>>>>>x>>>>>>>>>>>>>>>C");
        assert_eq!(res.auth, AuthType::S802_1X);
        assert_eq!(res.ip, Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(res.mac.octets(), [0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6]);
        assert_eq!(res.rssi, -85);
    }

    #[test]
    fn test_reads() {
        let buffer = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0xA, 1, 2, 3, 4];
        let reader = &mut buffer.as_slice();
        assert!(matches!(read32be(reader), Ok(0x01020304)));
        assert_eq!(read32be(reader).unwrap(), 0x05060708);
        assert_eq!(read16(reader).unwrap(), 0x0A09);
        assert_eq!(read32le(reader).unwrap(), 0x04030201);
        assert!(matches!(read8(reader), Err(_)));
    }

    #[test]
    fn test_ping_rep() {
        let reply = [
            1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 6, 2, 8, 9, 10,
        ];
        let f = read_ping_reply(&reply).unwrap();
        assert_eq!(f.0, Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(f.1, 0x05060708);
        assert_eq!(f.2, 0x02010A09);
        assert_eq!(f.3, 0x0403);
        assert_eq!(f.4, 0x0605);
        assert_eq!(f.5, PingError::Timeout);
        assert!(matches!(read_ping_reply(&[0u8; 19]), Err(_)));
    }

    #[test]
    fn test_read_dhcp_conf() {
        let reply = [
            192, 168, 1, 50, 192, 168, 1, 1, 192, 168, 1, 2, 255, 255, 255, 0, 0, 2, 0, 0,
        ];
        let conf = read_dhcp_conf(&reply).unwrap();
        assert_eq!(
            conf,
            IPConf {
                ip: Ipv4Addr::new(192, 168, 1, 50),
                gateway: Ipv4Addr::new(192, 168, 1, 1),
                dns: Ipv4Addr::new(192, 168, 1, 2),
                subnet: Ipv4Addr::new(255, 255, 255, 0),
                lease_time: 512
            }
        );
        assert!(matches!(read_dhcp_conf(&[0u8; 19]), Err(_)));
    }

    #[test]
    fn test_read_accept() {
        let mut reply = [
            2, 0, // address family
            0xBE, 0xDA, // port
            192, 168, 1, 50, // ip
            2,  // socket 1
            3,  // socket 2
            1, 2,
        ];
        let a = read_accept_reply(&reply).unwrap();
        assert_eq!(
            a.0,
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 0xBEDA)
        );
        assert_eq!(a.1, Socket::new(2, 0));
        assert_eq!(a.2, Socket::new(3, 0));
        assert_eq!(a.3, 513);
        reply[1] = 1;
        assert!(matches!(
            read_accept_reply(&reply),
            Err(Error::UnexpectedAddressFamily)
        ));
        assert!(matches!(read_accept_reply(&[0u8; 11]), Err(_)));
    }

    #[test]
    fn test_read_dns_reply() {
        let mut buffer = [65; 68];
        buffer[4] = 0;
        buffer[64] = 192;
        buffer[65] = 168;
        buffer[66] = 1;
        buffer[67] = 50;
        let rep = read_dns_reply(&buffer).unwrap();
        assert_eq!(rep.0, Ipv4Addr::new(192, 168, 1, 50));
        assert_eq!(&rep.1, "AAAA");
        assert!(matches!(read_dns_reply(&[0u8; 67]), Err(_)));
    }

    #[test]
    fn test_send_reply() {
        let buffer = [
            7,   //socket
            255, // unused
            1, 2, // bytes sent
            3, 4, // session
            255, 255,
        ]; // unused
        let rep = read_send_reply(&buffer).unwrap();
        assert_eq!(rep.0, Socket::new(7, 1027));
        assert_eq!(rep.1, 513);
        assert!(matches!(read_send_reply(&[0u8; 7]), Err(_)));
    }

    #[test]
    fn test_recv_reply() {
        let mut buffer = [
            2u8, 0, // AF_INET
            0xDE, 0xFA, // port
            192, 168, 1, 50, // ip
            10, 1, // status
            10, 2,   // offset
            7,   // socket
            255, // unused
            3, 4, // session
        ];
        let rep = read_recv_reply(&buffer).unwrap();
        assert_eq!(rep.0, Socket::new(7, 1027));
        assert_eq!(
            rep.1,
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 50), 0xDEFA)
        );
        assert_eq!(rep.2, 266);
        assert_eq!(rep.3, 522);

        buffer[1] = 1;
        assert!(matches!(
            read_recv_reply(&buffer),
            Err(Error::UnexpectedAddressFamily)
        ));
        assert!(matches!(read_recv_reply(&[15]), Err(_)));
    }

    #[test]
    fn test_common_reply() {
        let mut buffer = [7, 0, 3, 4];
        assert_eq!(
            read_common_socket_reply(&buffer).unwrap(),
            (Socket::new(7, 1027), SocketError::NoError)
        );
        buffer[1] = 254;
        assert_eq!(
            read_common_socket_reply(&buffer).unwrap(),
            (Socket::new(7, 1027), SocketError::AddrAlreadyInUse)
        );
        assert!(matches!(read_common_socket_reply(&[0]), Err(_)))
    }

    #[test]
    fn test_prng_reply() {
        let buffer = [0xDC, 0x65, 0x00, 0x20, 0x20, 0x00, 0x00, 0x00];
        assert_eq!(read_prng_reply(&buffer).unwrap(), (0x200065DC, 32))
    }

    #[test]
    fn test_provisioning_reply() {
        let buffer = [
            116, 101, 115, 116, 95, 115, 115, 105, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 116, 101, 115, 116, 95, 112, 97, 115, 115, 119, 111, 114,
            100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0,
        ];
        let info = read_provisioning_reply(&buffer).unwrap();

        assert_eq!(info.0.as_str(), "test_ssid");
        assert_eq!(info.1.as_str(), "test_password");
        assert_eq!(info.2, 2);
        assert_eq!(info.3, true);
    }

    #[test]
    fn test_connect_reply() {
        let buffer = [2, 255, 101, 0];

        let result = read_connect_socket_reply(&buffer);

        assert!(result.is_ok());

        let (sock, err) = result.unwrap();

        assert_eq!(sock.v, 2);
        assert_eq!(err, SocketError::InvalidAddress);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_connect_ssl_reply() {
        let buffer = [5, 243, 101, 101];

        let result = read_connect_socket_reply(&buffer);

        assert!(result.is_ok());

        let (sock, err) = result.unwrap();

        assert_eq!(sock.v, 5);
        assert_eq!(sock.s, 0);
        assert_eq!(err, SocketError::Timeout);
        assert_eq!(sock.get_ssl_data_offset(), 25957);
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_client_ecdh_response() {
        let buffer = [
            1, 0, /* ecc request type */
            52, 52, /* status */
            20, 40, 20, 40, /* user data */
            1, 1, 1, 1, /* sequence number */
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, /* x-coordinate */
            11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11,
            11, 11, 11, 11, 11, 11, 11, 11, 11, 11, /* y-coordinate */
            12, 12, /* point size */
            13, 13, /* private key id */
            14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
            14, 14, 14, 14, 14, 14, 14, 14, 14, 14, /* private key */
        ];

        let result = read_ssl_ecc_response(&buffer);
        assert!(result.is_ok());
        // ecc request
        let ecc_req = result.unwrap();
        // ecc request type
        assert_eq!(ecc_req.ecc_info.req, EccRequestType::ClientEcdh);
        // status
        assert_eq!(ecc_req.ecc_info.status, 13364);
        // user data
        assert_eq!(ecc_req.ecc_info.user_data, 672409620);
        // sequence number
        assert_eq!(ecc_req.ecc_info.seq_num, 16843009);
        // ecdh info opt
        assert!(ecc_req.ecdh_info.is_some());
        // ecdsa sign info opt
        assert!(ecc_req.ecdsa_sign_info.is_none());
        // ecdsa verify info opt
        assert!(ecc_req.ecdsa_verify_info.is_none());
        // ecdh info
        let ecdh_info = ecc_req.ecdh_info.unwrap();
        // x-coordinate
        let buffer = [10u8; 32];
        assert_eq!(ecdh_info.ecc_point.x_pos, buffer);
        // y-coordinate
        let buffer = [11u8; 32];
        assert_eq!(ecdh_info.ecc_point.y_pos, buffer);
        // point size
        assert_eq!(ecdh_info.ecc_point.point_size, 3084);
        // private key id
        assert_eq!(ecdh_info.ecc_point.private_key_id, 3341);
        // private key
        let buffer = [14u8; 32];
        assert_eq!(ecdh_info.private_key, buffer);
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_ecdsa_sign_response() {
        let buffer = [
            4, 0, /* ecc request type */
            52, 52, /* status */
            20, 40, 20, 40, /* user data */
            1, 2, 3, 4, /* sequence number */
            19, 0, /* Curve type */
            15, 16, /* hash size */
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let result = read_ssl_ecc_response(&buffer);
        assert!(result.is_ok());
        // ecc request
        let ecc_req = result.unwrap();
        // ecc request type
        assert_eq!(ecc_req.ecc_info.req, EccRequestType::GenerateSignature);
        // status
        assert_eq!(ecc_req.ecc_info.status, 13364);
        // user data
        assert_eq!(ecc_req.ecc_info.user_data, 672409620);
        // sequence number
        assert_eq!(ecc_req.ecc_info.seq_num, 67305985);
        // ecdh info opt
        assert!(ecc_req.ecdh_info.is_none());
        // ecdsa sign info opt
        assert!(ecc_req.ecdsa_sign_info.is_some());
        // ecdsa verify info opt
        assert!(ecc_req.ecdsa_verify_info.is_none());
        // ecdsa sign info
        let ecdsa_info = ecc_req.ecdsa_sign_info.unwrap();
        // curve type
        assert_eq!(ecdsa_info.curve_type, EccCurveType::Secp192r1);
        // hash size
        assert_eq!(ecdsa_info.hash_size, 4111);
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_ecdsa_verify_response() {
        let buffer = [
            5, 0, /* ecc request type */
            52, 52, /* status */
            20, 40, 20, 40, /* user data */
            4, 4, 2, 2, /* sequence number */
            19, 20, 21, 22, /* ecdsa verify */
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let result = read_ssl_ecc_response(&buffer);
        assert!(result.is_ok());
        // ecc request
        let ecc_req = result.unwrap();
        // ecc request type
        assert_eq!(ecc_req.ecc_info.req, EccRequestType::VerifySignature);
        // status
        assert_eq!(ecc_req.ecc_info.status, 13364);
        // user data
        assert_eq!(ecc_req.ecc_info.user_data, 672409620);
        // sequence number
        assert_eq!(ecc_req.ecc_info.seq_num, 33686532);
        // ecdh info opt
        assert!(ecc_req.ecdh_info.is_none());
        // ecdsa sign info opt
        assert!(ecc_req.ecdsa_sign_info.is_none());
        // ecdsa verify info opt
        assert!(ecc_req.ecdsa_verify_info.is_some());
        // ecdsa verify info
        assert_eq!(ecc_req.ecdsa_verify_info.unwrap(), 370480147);
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_ecc_response_unknown() {
        let buffer = [
            1, 1, /* ecc request type */
            52, 52, /* status */
            20, 40, 20, 40, /* user data */
            4, 4, 2, 2, /* sequence number */
            19, 20, 21, 22, /* ecdsa verify */
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let result = read_ssl_ecc_response(&buffer);
        assert!(result.is_err());
    }

    #[cfg(feature = "experimental-ecc")]
    #[test]
    fn test_ssl_ecc_response_no_request() {
        let buffer = [
            0, 0, /* ecc request type */
            52, 52, /* status */
            20, 40, 20, 40, /* user data */
            4, 4, 2, 2, /* sequence number */
            19, 20, 21, 22, /* ecdsa verify */
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let result = read_ssl_ecc_response(&buffer);
        assert!(result.is_ok());
        // ecc request
        let ecc_req = result.unwrap();
        // ecc request type
        assert_eq!(ecc_req.ecc_info.req, EccRequestType::None);
        // status
        assert_eq!(ecc_req.ecc_info.status, 13364);
        // user data
        assert_eq!(ecc_req.ecc_info.user_data, 672409620);
        // sequence number
        assert_eq!(ecc_req.ecc_info.seq_num, 33686532);
    }

    #[cfg(feature = "ethernet")]
    #[test]
    fn test_read_ethernet_reply() {
        let response = [25, 25, 00, 31];

        let result = read_eth_rx_reply(&response);
        assert!(result.is_ok());

        let (packet_size, data_offset) = result.unwrap();
        assert_eq!(packet_size, 6425 as u16);
        assert_eq!(data_offset, 7936 as u16)
    }
}
