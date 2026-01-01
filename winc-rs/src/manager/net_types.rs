// Copyright 2025 Google LLC
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

use crate::StackError;

use arrayvec::ArrayString;

use super::constants::{
    AuthType, WifiChannel, MAX_HOST_NAME_LEN, MAX_PSK_KEY_LEN, MAX_S802_PASSWORD_LEN,
    MAX_S802_USERNAME_LEN, MAX_SSID_LEN, MAX_WEP_KEY_LEN, MIN_PSK_KEY_LEN,
};

#[cfg(feature = "experimental-ecc")]
use super::constants::{EccCurveType, EccRequestType};

#[cfg(feature = "wep")]
use super::constants::{WepKeyIndex, MIN_WEP_KEY_LEN};
use core::net::Ipv4Addr;

/// Default IP address "192.168.1.1" for access point and provisioning mode.
const DEFAULT_AP_IP: u32 = 0xC0A80101;
#[cfg(feature = "experimental-ecc")]
const ECC_POINT_MAX_SIZE: usize = 32;
/// Maximum number of octets in Mac Address
const MAX_OCTETS_IN_MAC_ADDRESS: usize = 6;

/// Device Domain name.
pub type HostName = ArrayString<MAX_HOST_NAME_LEN>;
/// Wifi SSID
pub type Ssid = ArrayString<MAX_SSID_LEN>;
/// WPA-PSK key
pub type WpaKey = ArrayString<MAX_PSK_KEY_LEN>;
/// Wep Key
pub type WepKey = ArrayString<MAX_WEP_KEY_LEN>;
/// S802_1X Username
pub type S8Username = ArrayString<MAX_S802_USERNAME_LEN>;
/// S802_1X Password
pub type S8Password = ArrayString<MAX_S802_PASSWORD_LEN>;
/// ECDSA Verify Information.
#[cfg(feature = "experimental-ecc")]
pub(crate) type EcdsaVerifyInfo = u32;

/// Wi-Fi Security Credentials.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Credentials {
    Open = 1,
    /// WPA-PSK Passphrase: Must be at least 8 bytes (MIN) and no more than 63 bytes long.
    WpaPSK(WpaKey) = 2,
    /// Wep Passphrase: Should be 10 bytes for 40-bit and 26 bytes for 104-bit.
    /// Wep key Index: Between 1 and 4.
    #[cfg(feature = "wep")]
    Wep(WepKey, WepKeyIndex) = 3,
    /// 802.1X Username: Should not be greater then 20 bytes.
    /// 802.1X Password: Should not be greater then 40 bytes.
    S802_1X(S8Username, S8Password) = 4,
}

/// Socket Options
#[derive(PartialEq, Eq)]
pub enum SocketOptions {
    Tcp(TcpSockOpts),
    Udp(UdpSockOpts),
}

/// Socket Options
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum UdpSockOpts {
    /// Receive Timeout
    ReceiveTimeout(u32) = 0xff,
    /// Enable/Disable callback for UDP send.
    SetUdpSendCallback(bool) = 0x00,
    /// Join Multicast group
    JoinMulticast(Ipv4Addr) = 0x01,
    /// Leave Multicast group
    LeaveMulticast(Ipv4Addr) = 0x02,
}

/// TCP Socket Options
#[repr(u8)]
#[derive(PartialEq, Eq)]
pub enum TcpSockOpts {
    /// Receive Timeout
    ReceiveTimeout(u32) = 0xff,
    /// SSL Socket Options
    #[cfg(feature = "ssl")]
    Ssl(SslSockOpts) = 0xfe,
}

/// TLS Socket Option
#[cfg(feature = "ssl")]
#[repr(u8)]
#[derive(PartialEq, Eq)]
// Note: The `Copy` trait is not implemented to avoid
// copying the `ArrayString` (HostName).
pub enum SslSockOpts {
    /// SSL Configuration Options
    Config(SslSockConfig, bool),
    /// Server Name Indication (SNI).
    SetSni(HostName) = 0x02,
}

/// TLS Socket Configuration
#[cfg(feature = "ssl")]
#[repr(u8)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum SslSockConfig {
    /// Enable SSL.
    EnableSSL = 0x21, // 0x01 | 0x20 (SSL_FLAGS_NO_TX_COPY)
    /// Bypass X.509 certificate verification.
    /// Not recommended for production environments.
    BypassX509Verification = 0x02,
    /// Enable session caching to allow TLS
    /// session resumption for faster future connections.
    EnableSessionCache = 0x10,
    /// Enable SNI validation against the server's
    /// certificate subject common name.
    EnableSniValidation = 0x40,
}

/// Structure for Provisioning Information.
pub struct ProvisioningInfo {
    /// The SSID (network name) of the network.
    pub ssid: Ssid,
    /// Credentials for network's security.
    pub key: Credentials,
    // Status of Provisioning.
    pub(crate) status: bool,
}

/// Structure for Access Point Configuration.
#[derive(Debug, PartialEq, Eq)]
pub struct AccessPoint<'a> {
    /// The SSID (network name) of the network.
    pub ssid: &'a Ssid,
    /// The passphrase (Wi-Fi key) for the network's security.
    pub key: Credentials,
    /// The channel number (1..14) or 255 for all channels used by the access point.
    pub channel: WifiChannel,
    /// Whether the SSID is hidden (true for hidden).
    pub ssid_hidden: bool,
    /// IP address for the access point. The last octet must be in the range 1 to 99,
    /// for example: 192.168.1.1 to 192.168.1.99.
    /// Invalid Ip: 192.168.1.0 or 192.168.1.100.
    pub ip: Ipv4Addr,
}

/// Elliptic Curve point representation.
#[cfg(feature = "experimental-ecc")]
#[derive(Default)]
pub struct EccPoint {
    /// The X-coordinate of the ecc point.
    pub x_pos: [u8; ECC_POINT_MAX_SIZE],
    /// The Y-coordinate of the ecc point.
    pub y_pos: [u8; ECC_POINT_MAX_SIZE],
    // Point size in bytes (for each of the coordinates).
    pub point_size: u16,
    /// ID for the corresponding private key.
    pub private_key_id: u16,
}

/// Elliptic Curve Diffie-Hellman (ECDH) information.
#[cfg(feature = "experimental-ecc")]
#[derive(Default)]
pub struct EcdhInfo {
    /// ECC public key.
    pub ecc_point: EccPoint,
    /// Pre master secret generated during operations of
    /// type `ECC_REQ_CLIENT_ECDH` and `ECC_REQ_SERVER_ECDH`.
    pub private_key: [u8; ECC_POINT_MAX_SIZE],
}

/// Elliptic Curve Digital Signature Algorithm (ECDSA) signing information.
#[cfg(feature = "experimental-ecc")]
#[derive(Default)]
pub struct EcdsaSignInfo {
    /// Key Curve Type (NIST P-256, P-384)
    pub curve_type: EccCurveType,
    /// Hash Size (32 bytes for SHA-256 or 48 bytes for SHA-384)
    pub hash_size: u16,
}

/// ECC Operations Info.
#[cfg(feature = "experimental-ecc")]
#[derive(Default)]
pub struct EccInfo {
    /// ECC Request type (ECDH, ECDSA Sign or ECDSA Verify)
    pub req: EccRequestType,
    /// Status of ECC operation.
    /// Value should be used in response and ignored in request.
    /// zero for success and non-zero for failure.
    pub status: u16,
    /// User data
    /// Value in response should match the value in request from WINC.
    pub user_data: u32,
    /// Sequence number
    /// Value in response to WINC should match the value in request from WINC.
    pub seq_num: u32,
}

/// ECC Request Information received from WINC.
#[cfg(feature = "experimental-ecc")]
#[derive(Default)]
pub(crate) struct EccRequest {
    pub hif_reg: u32,
    pub ecc_info: EccInfo,
    pub ecdh_info: Option<EcdhInfo>,
    pub ecdsa_sign_info: Option<EcdsaSignInfo>,
    pub ecdsa_verify_info: Option<EcdsaVerifyInfo>,
}

/// SSL callback info
#[cfg(feature = "ssl")]
#[derive(Default)]
pub(crate) struct SslCallbackInfo {
    pub(crate) cipher_suite_bitmap: Option<Option<u32>>,
    #[cfg(feature = "experimental-ecc")]
    pub(crate) ecc_req: Option<EccRequest>,
}

/// Ethernet RX information.
#[cfg(feature = "ethernet")]
pub(crate) struct EthernetRxInfo {
    pub(crate) packet_size: u16,
    pub(crate) data_offset: u16,
    pub(crate) hif_address: u32,
}

/// MAC Address
pub struct MacAddress {
    mac: [u8; MAX_OCTETS_IN_MAC_ADDRESS],
}

/// Implementation to convert the Credentials to Authentication Type
impl From<Credentials> for AuthType {
    fn from(cred: Credentials) -> Self {
        match cred {
            Credentials::Open => Self::Open,
            Credentials::WpaPSK(_) => Self::WpaPSK,
            #[cfg(feature = "wep")]
            Credentials::Wep(_, _) => Self::WEP,
            Credentials::S802_1X(_, _) => Self::S802_1X,
        }
    }
}

/// Implementation to convert the Credentials to `u8` value.
impl From<Credentials> for u8 {
    fn from(val: Credentials) -> Self {
        match val {
            Credentials::Open => 1,
            Credentials::WpaPSK(_) => 2,
            #[cfg(feature = "wep")]
            Credentials::Wep(_, _) => 3,
            Credentials::S802_1X(_, _) => 4,
        }
    }
}

/// Implementation of `Credentials` to create new configuration or get length of stored key.
impl Credentials {
    /// Get the length of password stored in the Credentials.
    pub fn key_len(&self) -> usize {
        match self {
            Credentials::Open => 0,
            #[cfg(feature = "wep")]
            Credentials::Wep(key, _) => key.len(),
            Credentials::WpaPSK(key) => key.len(),
            Credentials::S802_1X(_, key) => key.len(),
        }
    }

    /// Generates WPA-PSK credentials from a password.
    ///
    /// # Arguments
    ///
    /// * `password` - The WPA-PSK passphrase. Must be at least 8 bytes and no more than 63 bytes long.
    ///
    /// # Returns
    ///
    /// * `Credentials::WpaPSK` - Configured WPA-PSK credentials on success.
    /// * `StackError` - If any parameter validation fails.
    pub fn from_wpa(password: &str) -> Result<Self, StackError> {
        if password.len() < MIN_PSK_KEY_LEN {
            return Err(StackError::InvalidParameters);
        }

        WpaKey::from(password)
            .map(Self::WpaPSK)
            .map_err(|_| StackError::InvalidParameters)
    }

    /// Generates 802.1X credentials from a username and password.
    ///
    /// # Arguments
    ///
    /// * `username` - The RADIUS server username. Must not exceed 20 bytes.
    /// * `password` - The RADIUS server password. Must not exceed 40 bytes.
    ///
    /// # Returns
    ///
    /// * `Credentials::S802_1X` - Configured 802.1X credentials on success.
    /// * `StackError` - If any parameter validation fails.
    pub fn from_s802(username: &str, password: &str) -> Result<Self, StackError> {
        let username = S8Username::from(username).map_err(|_| StackError::InvalidParameters)?;
        let password = S8Password::from(password).map_err(|_| StackError::InvalidParameters)?;
        Ok(Self::S802_1X(username, password))
    }

    #[cfg(feature = "wep")]
    /// Generates WEP credentials from a WEP key and key index.
    ///
    /// # Arguments
    ///
    /// * `key` - The WEP key. Must be 10 bytes for 40-bit or 26 bytes for 104-bit.
    /// * `key_index` - The index of the WEP key to use.
    ///
    /// # Returns
    ///
    /// * `Ok(Credentials::WEP)` - Configured WEP credentials on success.
    /// * `Err(StackError)` - If parameter validation fails.
    pub fn from_wep(key: &str, key_index: WepKeyIndex) -> Result<Self, StackError> {
        if key.len() != MAX_WEP_KEY_LEN && key.len() != MIN_WEP_KEY_LEN {
            return Err(StackError::InvalidParameters);
        }

        WepKey::from(key)
            .map(|key| Credentials::Wep(key, key_index))
            .map_err(|_| StackError::InvalidParameters)
    }
}

/// Implementation to convert `UdpSockOpts` to `u8` value.
impl From<UdpSockOpts> for u8 {
    fn from(value: UdpSockOpts) -> Self {
        match value {
            UdpSockOpts::SetUdpSendCallback(_) => 0x00,
            UdpSockOpts::JoinMulticast(_) => 0x01,
            UdpSockOpts::LeaveMulticast(_) => 0x02,
            UdpSockOpts::ReceiveTimeout(_) => 0xff,
        }
    }
}

/// Implementation to get 32-bit value stored in UDP socket option.
impl UdpSockOpts {
    /// Get the value of the Socket option.
    pub fn get_value(&self) -> u32 {
        match self {
            UdpSockOpts::ReceiveTimeout(val) => *val,
            UdpSockOpts::SetUdpSendCallback(val) => *val as u32,
            UdpSockOpts::JoinMulticast(val) | UdpSockOpts::LeaveMulticast(val) => {
                // Address needs to be in big endian format.
                u32::from_le_bytes(val.to_bits().to_be_bytes())
            }
        }
    }
}

/// Implementation to convert `TcpSockOpts` to `u8` value.
impl From<TcpSockOpts> for u8 {
    fn from(value: TcpSockOpts) -> Self {
        match value {
            #[cfg(feature = "ssl")]
            TcpSockOpts::Ssl(_) => 0xfe,
            TcpSockOpts::ReceiveTimeout(_) => 0xff,
        }
    }
}

/// Implementation to get 32-bit value stored in TCP socket option.
impl TcpSockOpts {
    /// Get the value of the Socket option.
    pub fn get_value(&self) -> u32 {
        match self {
            TcpSockOpts::ReceiveTimeout(val) => *val,
            // SSL values don't have 32 bit values.
            #[cfg(feature = "ssl")]
            TcpSockOpts::Ssl(_) => 0xfe,
        }
    }
}

/// Implementation to convert `SslSockOpts` to `u8` value.
#[cfg(feature = "ssl")]
impl From<SslSockOpts> for u8 {
    fn from(value: SslSockOpts) -> Self {
        match value {
            SslSockOpts::Config(cfg, _) => cfg.into(),
            SslSockOpts::SetSni(_) => 0x02,
        }
    }
}

/// Implementation to convert `&SslSockOpts` to `u8` value.
#[cfg(feature = "ssl")]
impl From<&SslSockOpts> for u8 {
    fn from(value: &SslSockOpts) -> Self {
        match value {
            SslSockOpts::Config(cfg, _) => (*cfg).into(),
            SslSockOpts::SetSni(_) => 0x02,
        }
    }
}

/// Implementation to convert `SslSockConfig` to `u8` value.
#[cfg(feature = "ssl")]
impl From<SslSockConfig> for u8 {
    fn from(value: SslSockConfig) -> Self {
        value as Self
    }
}

/// Implementation to get value stored in SSL socket option.
#[cfg(feature = "ssl")]
impl SslSockOpts {
    /// Gets the SNI value passed in the `SslSockOpts::SetSni` socket option.
    ///
    /// # Returns
    ///
    /// * `HostName` – The stored hostname as a string array.
    /// * `StackError` – If the method is called for any SSL socket option other than `SetSni`.
    pub(crate) fn get_sni_value(&self) -> Result<&HostName, StackError> {
        match self {
            SslSockOpts::SetSni(hostname) => Ok(hostname),
            _ => Err(StackError::InvalidParameters),
        }
    }
}

/// Implementation to create Socket Option configuration
impl SocketOptions {
    /// Set the socket option to join an IPv4 multicast group.
    ///
    /// # Arguments
    ///
    /// * `addr` - The IPv4 multicast address to join.
    ///
    /// # Returns
    ///
    /// * `SocketOption::Udp(UdpSockOpts::JoinMulticast` - The configured socket option.
    pub fn join_multicast_v4(addr: Ipv4Addr) -> Self {
        Self::Udp(UdpSockOpts::JoinMulticast(addr))
    }

    /// Set the socket option to leave an IPv4 multicast group.
    ///
    /// # Arguments
    ///
    /// * `addr` - The IPv4 multicast address to leave.
    ///
    /// # Returns
    ///
    /// * `SocketOption::Udp(UdpSockOpts::LeaveMulticast` - The configured socket option.
    pub fn leave_multicast_v4(addr: Ipv4Addr) -> Self {
        Self::Udp(UdpSockOpts::LeaveMulticast(addr))
    }

    /// Set the socket option to enable or disable the UDP send callback.
    ///
    /// # Arguments
    ///
    /// * `status` - Whether to enable (`true`) or disable (`false`) the UDP send callback.
    ///
    /// # Returns
    ///
    /// * `SocketOption::Udp(UdpSockOpts::SetUdpSendCallback` - The configured socket option.
    pub fn set_udp_send_callback(status: bool) -> Self {
        Self::Udp(UdpSockOpts::SetUdpSendCallback(status))
    }

    /// Set a socket option to configure the UDP socket receive timeout.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Timeout duration in milliseconds.
    ///
    /// # Returns
    ///
    /// * `SocketOption::Udp(UdpSockOpts::ReceiveTimeout` - The configured socket option.
    pub fn set_udp_receive_timeout(timeout: u32) -> Self {
        Self::Udp(UdpSockOpts::ReceiveTimeout(timeout))
    }

    /// Set a socket option to configure the TCP socket receive timeout.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Timeout duration in milliseconds.
    ///
    /// # Returns
    ///
    /// * `SocketOption::Tcp(TcpSockOpts::ReceiveTimeout)` - The configured socket option.
    pub fn set_tcp_receive_timeout(timeout: u32) -> Self {
        Self::Tcp(TcpSockOpts::ReceiveTimeout(timeout))
    }

    /// Set the socket option to configure SNI (Server Name Indication) for TLS connections.
    ///
    /// # Arguments
    ///
    /// * `hostname` - The hostname to be used for SNI. Must not be greater then 63 bytes.
    ///
    /// # Returns
    ///
    /// * `SocketOptions` – The configured socket option on success.
    /// * `StackError` – If the hostname length is invalid.
    #[cfg(feature = "ssl")]
    pub fn set_sni(hostname: &str) -> Result<Self, StackError> {
        let host = HostName::from(hostname).map_err(|_| StackError::InvalidParameters)?;
        Ok(Self::Tcp(TcpSockOpts::Ssl(SslSockOpts::SetSni(host))))
    }

    /// Sets the socket option to configure SSL socket.
    ///
    /// # Arguments
    ///
    /// * `cfg` – The SSL socket configuration.
    /// * `enable` – `true` to enable the configuration, or `false` to disable it.
    ///
    /// # Returns
    ///
    /// * `SocketOptions` – The configured socket option on success.
    #[cfg(feature = "ssl")]
    pub fn config_ssl(cfg: SslSockConfig, enable: bool) -> Self {
        Self::Tcp(TcpSockOpts::Ssl(SslSockOpts::Config(cfg, enable)))
    }
}

impl<'a> AccessPoint<'a> {
    /// Creates a new access point configuration with the provided parameters.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name) up to 32 characters.
    /// * `key` - Security credentials depends on the `auth` type.
    /// * `auth` - The authentication method (e.g., Open, WPA2).
    /// * `channel` - The Wi-Fi channel to operate on (typically between 1 and 14).
    /// * `ssid_hidden` - Whether the SSID should be hidden from network scans (true for hidden).
    /// * `ip` - The static IPv4 address to assign to the access point.
    ///
    /// # Notes
    ///
    /// For Open, the security type should be empty.
    /// For WPA, the security key must be at least 8 bytes (MIN) and no more than 63 bytes long.
    /// For WEP, the security key should be 10 bytes for 40-bit and 26 bytes for 104-bit.
    /// For S802, the security key should be no more then 40 bytes long.
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - Configured access point structure on success.
    /// * `StackError` - If validation of any parameters fails.
    pub fn new(
        ssid: &'a Ssid,
        key: Credentials,
        channel: WifiChannel,
        ssid_hidden: bool,
        ip: Ipv4Addr,
    ) -> Result<Self, StackError> {
        let octets = ip.octets();
        let auth = <Credentials as Into<AuthType>>::into(key);

        if !((1..100).contains(&octets[3])) {
            return Err(StackError::InvalidParameters);
        }

        if auth == AuthType::S802_1X {
            return Err(StackError::InvalidParameters);
        }

        Ok(Self {
            ssid,
            key,
            channel,
            ssid_hidden,
            ip: Ipv4Addr::from(octets),
        })
    }

    /// Creates configuration for an open (no security) access point.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name) string up to 32 bytes.
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - The configured `AccessPoint` with open (no security) on success.
    pub fn open(ssid: &'a Ssid) -> Self {
        Self {
            ssid,
            key: Credentials::Open,
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::from_bits(DEFAULT_AP_IP),
        }
    }

    #[cfg(feature = "wep")]
    /// Creates configuration for a WEP-secured access point.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    /// * `key` - The WEP security key, either 10 bytes (for 40-bit) or 26 bytes (for 104-bit).
    /// * `key_index` - Wep Key Index; typically between 0 and 4.
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - The configured `AccessPoint` with WEP security on success.
    pub fn wep(ssid: &'a Ssid, key: &'a WepKey, key_index: WepKeyIndex) -> Self {
        Self {
            ssid,
            key: Credentials::Wep(*key, key_index),
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::from_bits(DEFAULT_AP_IP),
        }
    }

    /// Creates a configuration for a WPA or WPA2-secured access point.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    /// * `key` - The WPA security key, up to 63 bytes (as per WPA/WPA2 specification).
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - The configured `AccessPoint` with WPA-PSK security on success.
    pub fn wpa(ssid: &'a Ssid, key: &'a WpaKey) -> Self {
        Self {
            ssid,
            key: Credentials::WpaPSK(*key),
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::from_bits(DEFAULT_AP_IP),
        }
    }

    /// Sets the static IP address for the configured access point.
    ///
    /// # Arguments
    ///
    /// * `ip` - The new static IPv4 address to assign to the access point.
    ///
    /// # Warning
    ///
    /// Due to a WINC firmware limitation, the access point IP address can only be in the range `x.x.x.1` to `x.x.x.99`.
    ///
    /// # Returns
    ///
    /// * `()` - If the IP address is successfully set.
    /// * `StackError` - If the IP address is invalid.
    pub fn set_ip(&mut self, ip: Ipv4Addr) -> Result<(), StackError> {
        let octets = ip.octets();
        // WINC firmware limitation; IP address of client is always x.x.x.100
        if !((1..100).contains(&octets[3])) {
            return Err(StackError::InvalidParameters);
        }

        self.ip = Ipv4Addr::from(octets);

        Ok(())
    }

    /// Sets the Wi-Fi channel for the configured access point.
    ///
    /// # Arguments
    ///
    /// * `channel` - The Wi-Fi RF channel to use (typically 1–14).
    pub fn set_channel(&mut self, channel: WifiChannel) {
        self.channel = channel;
    }

    /// Sets whether the SSID is hidden.
    ///
    /// # Arguments
    ///
    /// * `hidden` – `true` to hide the SSID, `false` to make it visible.
    pub fn set_ssid_hidden(&mut self, hidden: bool) {
        self.ssid_hidden = hidden;
    }
}

impl MacAddress {
    /// Creates a new `MacAddress` from six individual octets.
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> MacAddress {
        Self {
            mac: [a, b, c, d, e, f],
        }
    }

    /// Creates a MAC address from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StackError> {
        if bytes.len() != MAX_OCTETS_IN_MAC_ADDRESS {
            return Err(StackError::InvalidParameters);
        }

        let mut mac = [0u8; MAX_OCTETS_IN_MAC_ADDRESS];
        mac.copy_from_slice(bytes);

        Ok(Self { mac })
    }

    /// Creates an empty MAC address with all octets set to zero.
    pub fn empty() -> Self {
        Self {
            mac: [0u8; MAX_OCTETS_IN_MAC_ADDRESS],
        }
    }

    /// Returns the MAC address as a fixed-size array of octets.
    pub fn octets(&self) -> [u8; MAX_OCTETS_IN_MAC_ADDRESS] {
        self.mac
    }

    /// Returns a mutable slice of the MAC address bytes.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.mac
    }

    /// Returns a immutable slice of the MAC address bytes.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.mac
    }
}

/// Implements the `core::fmt::Display` trait to display the MAC address in
/// hexadecimal format, with octets separated by colons.
/// For example: `AA:BB:CC:DD:EE:FF`.
#[cfg(feature = "log")]
impl core::fmt::Display for MacAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, byte) in self.mac.iter().enumerate() {
            if i != 0 {
                write!(f, ":")?;
            }
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

/// Implements the `defmt::Format` trait to display the MAC address in
/// hexadecimal format, with octets separated by colons.
/// For example: `AA:BB:CC:DD:EE:FF`.
#[cfg(feature = "defmt")]
impl defmt::Format for MacAddress {
    fn format(&self, f: defmt::Formatter) {
        for (i, byte) in self.mac.iter().enumerate() {
            if i != 0 {
                defmt::write!(f, ":");
            }
            defmt::write!(f, "{:02X}", byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;

    #[test]
    fn test_ap_set_channel() {
        let ssid = Ssid::from("test").unwrap();
        let mut ap = AccessPoint::open(&ssid);

        assert_eq!(ap.channel, WifiChannel::Channel1);

        ap.set_channel(WifiChannel::Channel2);

        assert_eq!(ap.channel, WifiChannel::Channel2);
    }

    #[test]
    fn test_ap_set_ip_fail() {
        let ssid = Ssid::from("test").unwrap();
        let psk = WpaKey::from("test_key").unwrap();
        let mut ap = AccessPoint::wpa(&ssid, &psk);
        let ip = Ipv4Addr::new(192, 168, 1, 100);

        assert_eq!(ap.ip, Ipv4Addr::from_bits(DEFAULT_AP_IP));

        let result = ap.set_ip(ip);

        assert_eq!(result, Err(StackError::InvalidParameters))
    }

    #[test]
    fn test_ap_set_ip_success() {
        let ssid = Ssid::from("test").unwrap();
        let psk = WpaKey::from("test_key").unwrap();
        let mut ap = AccessPoint::wpa(&ssid, &psk);
        let ip = Ipv4Addr::new(192, 168, 1, 1);

        assert_eq!(ap.ip, Ipv4Addr::from_bits(DEFAULT_AP_IP));

        let result = ap.set_ip(ip);

        assert!(result.is_ok());
    }

    #[test]
    fn test_ap_config_fail() {
        let ssid = Ssid::from("test").unwrap();
        let psk = WpaKey::from("test_key").unwrap();
        // Access Point Configuration.
        let ap = AccessPoint::new(
            &ssid,
            Credentials::WpaPSK(psk),
            WifiChannel::Channel1,
            false,
            Ipv4Addr::new(192, 168, 1, 100),
        );

        assert_eq!(ap, Err(StackError::InvalidParameters));
    }

    #[test]
    fn test_ap_config_success() {
        let ssid = Ssid::from("test").unwrap();
        let psk = WpaKey::from("test_key").unwrap();
        // Access Point Configuration.
        let ap = AccessPoint::new(
            &ssid,
            Credentials::WpaPSK(psk),
            WifiChannel::Channel1,
            false,
            Ipv4Addr::new(192, 168, 1, 1),
        );

        assert!(ap.is_ok());
    }

    #[test]
    fn test_ap_config_enterprise_fail() {
        let ssid = Ssid::from("test").unwrap();
        let username = S8Username::from("username").unwrap();
        let password = S8Password::from("password").unwrap();
        // Access Point Configuration.
        let ap = AccessPoint::new(
            &ssid,
            Credentials::S802_1X(username, password),
            WifiChannel::Channel1,
            false,
            Ipv4Addr::new(192, 168, 1, 1),
        );

        assert_eq!(ap, Err(StackError::InvalidParameters));
    }

    #[test]
    fn test_ssid_visibility() {
        let ssid = Ssid::from("test").unwrap();
        let mut ap = AccessPoint::open(&ssid);

        assert_eq!(ap.ssid_hidden, false);

        ap.set_ssid_hidden(true);

        assert_eq!(ap.ssid_hidden, true);
    }

    #[test]
    fn test_wpa_credentials_with_short_password() {
        let result = Credentials::from_wpa("pass");
        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[test]
    fn test_wpa_credentials_with_long_password() {
        let long_password = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let result = Credentials::from_wpa(long_password);
        assert!(result.is_err());
    }

    #[test]
    fn test_wpa_credentials_with_valid_password() {
        let result = Credentials::from_wpa("ABCDEFGHIJKLM");
        assert!(result.is_ok());
    }

    #[test]
    fn test_s802_credentials_invalid_username() {
        let username = "abcdefghijklmnopqrst123";
        let password = "abcdefghijklmnopqrstuvwxyz1234567890ABCD";
        let result = Credentials::from_s802(username, password);
        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[test]
    fn test_s802_credentials_invalid_password() {
        let username = "abcdefghijklmnopqrst";
        let password = "abcdefghijklmnopqrstuvwxyz1234567890ABC123D";
        let result = Credentials::from_s802(username, password);
        assert_eq!(result.err(), Some(StackError::InvalidParameters));
    }

    #[test]
    fn test_s802_credentials_valid() {
        let username = "abcdefghijklmnopqrst";
        let password = "abcdefghijklmnopqrstuvwxyz1234567890ABCD";
        let result = Credentials::from_s802(username, password);
        assert!(result.is_ok());
    }

    #[test]
    fn test_s802_key_len() {
        let username = "abcdefghijklmnopqrst";
        let password = "abcdefghijklmnopqrstuvwxyz1234567890ABCD";
        let result = Credentials::from_s802(username, password);
        assert!(result.is_ok());

        assert_eq!(result.unwrap().key_len(), password.len());
    }

    #[test]
    fn test_s802_auth_type() {
        let username = "abcdefghijklmnopqrst";
        let password = "abcdefghijklmnopqrstuvwxyz1234567890ABCD";
        let result = Credentials::from_s802(username, password);
        assert!(result.is_ok());

        let s802_auth: u8 = result.unwrap().into();

        assert_eq!(s802_auth, <AuthType as Into<u8>>::into(AuthType::S802_1X));
    }

    #[cfg(feature = "wep")]
    #[test]
    fn test_wep_credentials_with_small_key() {
        let result = Credentials::from_wep("ABCDEFG", WepKeyIndex::Key1);
        assert!(result.is_err());
    }

    #[cfg(feature = "wep")]
    #[test]
    fn test_wep_credentials_with_large_key() {
        let long_password = "ABCDEFGlmnopqrstuvwxyz0123456789+/";
        let result = Credentials::from_wep(long_password, WepKeyIndex::Key2);
        assert!(result.is_err());
    }

    #[cfg(feature = "wep")]
    #[test]
    fn test_wep_credentials_with_104_bit_key() {
        let key = "0123456789ABCDEF0123456789";
        let result = Credentials::from_wep(key, WepKeyIndex::Key3);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sock_opts_get_join_multicast_value_success() {
        let test_value: u32 = 0x101a8c0;
        let addr = Ipv4Addr::from_str("192.168.1.1").unwrap();
        let sock_opt = SocketOptions::join_multicast_v4(addr);

        if let SocketOptions::Udp(opt) = sock_opt {
            assert_eq!(test_value, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_get_join_multicast_value_fail() {
        let test_value: u32 = 0xc0a80101;
        let addr = Ipv4Addr::from_str("192.168.1.1").unwrap();
        let sock_opt = SocketOptions::join_multicast_v4(addr);

        if let SocketOptions::Udp(opt) = sock_opt {
            assert_ne!(test_value, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_get_leave_multicast_value_success() {
        let test_value: u32 = 0x101a8c0;
        let addr = Ipv4Addr::from_str("192.168.1.1").unwrap();
        let sock_opt = SocketOptions::join_multicast_v4(addr);

        if let SocketOptions::Udp(opt) = sock_opt {
            assert_eq!(test_value, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_get_leave_multicast_value_fail() {
        let test_value: u32 = 0xc0a80101;
        let addr = Ipv4Addr::from_str("192.168.1.1").unwrap();
        let sock_opt = SocketOptions::leave_multicast_v4(addr);

        if let SocketOptions::Udp(opt) = sock_opt {
            assert_ne!(test_value, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_get_udp_send_callback_false_value() {
        let sock_opts = SocketOptions::set_udp_send_callback(false);

        if let SocketOptions::Udp(opt) = sock_opts {
            assert_eq!(0u32, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_get_udp_send_callback_true_value() {
        let sock_opts = SocketOptions::set_udp_send_callback(true);

        if let SocketOptions::Udp(opt) = sock_opts {
            assert_eq!(1u32, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_get_udp_recv_timeout_value() {
        let test_value = 1500u32;
        let sock_opts = SocketOptions::set_udp_receive_timeout(test_value);

        if let SocketOptions::Udp(opt) = sock_opts {
            assert_eq!(test_value, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_get_tcp_recv_timeout_value() {
        let test_value = 2500u32;
        let sock_opts = SocketOptions::set_tcp_receive_timeout(test_value);

        if let SocketOptions::Tcp(opt) = sock_opts {
            assert_eq!(test_value, opt.get_value());
        } else {
            assert!(false);
        }
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_sock_opts_get_sni_array_success() {
        let test_value = "hostname".as_bytes();
        let sock_opts = SocketOptions::set_sni("hostname").unwrap();

        if let SocketOptions::Tcp(opt) = sock_opts {
            if let TcpSockOpts::Ssl(ssl) = opt {
                assert_eq!(ssl.get_sni_value().unwrap().as_bytes(), test_value);
            } else {
                assert!(false);
            }
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_sock_opts_udp_join_multicast_u8_value() {
        let addr = Ipv4Addr::from_str("192.168.1.1").unwrap();
        let sock_opts = UdpSockOpts::JoinMulticast(addr);
        assert_eq!(u8::from(sock_opts), 0x01u8);
    }

    #[test]
    fn test_sock_opts_udp_leave_multicast_u8_value() {
        let addr = Ipv4Addr::from_str("192.168.1.1").unwrap();
        let sock_opts = UdpSockOpts::LeaveMulticast(addr);
        assert_eq!(u8::from(sock_opts), 0x02u8);
    }

    #[test]
    fn test_sock_opts_udp_receive_timeout_u8_value() {
        let sock_opts = UdpSockOpts::ReceiveTimeout(1500);
        assert_eq!(u8::from(sock_opts), 0xffu8);
    }

    #[test]
    fn test_sock_opts_udp_send_callback_u8_value() {
        let sock_opts = UdpSockOpts::SetUdpSendCallback(false);
        assert_eq!(u8::from(sock_opts), 0x00u8);
    }

    #[test]
    fn test_sock_opts_tcp_receive_timeout_u8_value() {
        let sock_opts = TcpSockOpts::ReceiveTimeout(1500);
        assert_eq!(u8::from(sock_opts), 0xffu8);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_sock_opts_tcp_ssl_u8_value() {
        let host = HostName::from("hostname").unwrap();
        let sock_opts = TcpSockOpts::Ssl(SslSockOpts::SetSni(host));
        assert_eq!(u8::from(sock_opts), 0xfeu8);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_sock_opts_get_tcp_ssl_sni_value() {
        let host = HostName::from("hostname").unwrap();
        let sock_opts = TcpSockOpts::Ssl(SslSockOpts::SetSni(host));
        assert_eq!(sock_opts.get_value(), 0xfeu32);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_sock_opts_ssl_sni_u8_value() {
        let host = HostName::from("hostname").unwrap();
        let sock_opts = SslSockOpts::SetSni(host);
        assert_eq!(u8::from(sock_opts), 0x02u8);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_sock_opts_ssl_sni_invalid_parameter() {
        let test_string =
            "This is a test string that definitely contains more than sixty-three bytes of data.";
        let sock_opts = SocketOptions::set_sni(test_string);
        assert_eq!(sock_opts.err(), Some(StackError::InvalidParameters));
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_ssl_sock_opt_conv() {
        let sock_opt = SslSockOpts::Config(SslSockConfig::EnableSessionCache, true);

        let u8_value = u8::from(sock_opt);

        assert_eq!(u8_value, u8::from(SslSockConfig::EnableSessionCache));
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_ssl_sock_opt_ref_conv() {
        let sock_opt = SslSockOpts::Config(SslSockConfig::EnableSniValidation, true);
        let sock_opt_ref = &sock_opt;

        let u8_value = u8::from(sock_opt_ref);

        assert_eq!(u8_value, u8::from(SslSockConfig::EnableSniValidation));
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_ssl_sock_opt_invalid_opt() {
        let sock_opt = SslSockOpts::Config(SslSockConfig::EnableSniValidation, true);

        let res = sock_opt.get_sni_value();

        assert_eq!(res, Err(StackError::InvalidParameters));
    }

    #[test]
    fn test_create_new_mac_address() {
        let mac = MacAddress::new(0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6);
        assert_eq!(mac.octets(), [0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6]);

        let mac = MacAddress::empty();
        assert_eq!(mac.octets(), [0u8; MAX_OCTETS_IN_MAC_ADDRESS]);
    }

    #[test]
    fn test_create_mac_address_from_array_success() {
        let bytes = [0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6];
        let mac = MacAddress::from_bytes(&bytes);

        assert!(mac.is_ok());
        assert_eq!(mac.unwrap().as_slice(), bytes);
    }

    #[test]
    fn test_create_mac_address_from_array_failure() {
        let bytes = [0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0xAA];
        let mac = MacAddress::from_bytes(&bytes);

        assert_eq!(mac.err(), Some(StackError::InvalidParameters));
    }

    #[test]
    fn test_create_mac_address_mut_array() {
        let bytes = [0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6];

        let mut mac = MacAddress::from_bytes(&bytes).unwrap();

        #[cfg(feature = "log")]
        log::info!("Mac Address: {}", mac);

        let mod_mac = mac.as_mut_slice();

        for (index, byte) in mod_mac.iter_mut().enumerate() {
            *byte = (index * 2) as u8;
        }

        assert_ne!(mod_mac, &bytes);
    }

    #[cfg(feature = "log")]
    #[test]
    fn test_format_mac_address() {
        use std::format;

        let bytes = [0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6];
        let mac = MacAddress::from_bytes(&bytes);

        assert_eq!(format!("{}", mac.unwrap()), "A1:B2:C3:D4:E5:F6");
    }
}
