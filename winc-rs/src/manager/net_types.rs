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

use crate::{errors::Error, StackError};

use arrayvec::ArrayString;

use super::constants::{
    AuthType, WifiChannel, MAX_HOST_NAME_LEN, MAX_PSK_KEY_LEN, MAX_S802_PASSWORD_LEN,
    MAX_S802_USERNAME_LEN, MAX_SSID_LEN,
};

use super::constants::MAX_WEP_KEY_LEN;
use core::net::Ipv4Addr;

// Default IP address for provisioning mode.
const PROVISIONING_DEFAULT_IP: u32 = 0xC0A80101;

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

/// Extension trait for ArrayString
pub trait ArrayStringExt<const N: usize> {
    /// Returns a `[u8; N]` containing the string bytes, padded with `0`s.
    fn as_padded_bytes(&self) -> [u8; N];
}

/// Wi-Fi Security Credentials.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Credentials {
    Open = 1,
    /// WPA-PSK Passpharase: Must be at least 8 bytes (MIN) and no more than 63 bytes long.
    WpaPSK(WpaKey) = 2,
    /// Wep Passphrase: Should be 10 characters for 40-bit and 26 characters for 104-bit.
    /// Wep key Index: Between 1 and 4.
    #[cfg(feature = "wep")]
    Wep(WepKey, u8) = 3,
    /// 802.1X enterprise authentication.
    ///
    /// Used for WPA/WPA2-Enterprise networks that require a username and password.
    ///
    /// # Fields
    /// - `S8Username`: The user identity
    /// - `S8Password`: The user password
    S802_1X(S8Username, S8Password) = 4,
}

/// Structure for Provisioning Information.
pub struct ProvisionalInfo {
    /// The SSID (network name) of the network.
    pub ssid: Ssid,
    /// Credentials for network's security.
    pub key: Credentials,
}

/// Structure for Access Point Configuration.
pub struct AccessPoint<'a> {
    /// The SSID (network name) of the network.
    pub ssid: &'a Ssid,
    /// The passphrase (Wi-Fi key) for the network's security.
    pub key: Credentials,
    /// The channel number (1..14) or 255 for all channels used by the access point.
    pub channel: WifiChannel,
    /// Whether the SSID is hidden (true for hidden).
    pub ssid_hidden: bool,
    /// IP address for the access point. The last octet must be in the range 1 to 100,
    /// for example: 192.168.1.1 to 192.168.1.100.
    pub ip: Ipv4Addr,
}

/// Implementation For ArrayStringExt Trait
impl<const N: usize> ArrayStringExt<N> for ArrayString<N> {
    fn as_padded_bytes(&self) -> [u8; N] {
        let mut buf = [0u8; N];
        let bytes = self.as_bytes();
        buf[..bytes.len()].copy_from_slice(bytes);
        buf
    }
}

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

impl Credentials {
    pub fn key_len(&self) -> usize {
        match self {
            Credentials::Open => 0,
            #[cfg(feature = "wep")]
            Credentials::Wep(key, _) => key.capacity(),
            Credentials::WpaPSK(key) => key.capacity(),
            Credentials::S802_1X(_, key) => key.capacity(),
        }
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
        if !((1..100).contains(&octets[3])) {
            return Err(StackError::WincWifiFail(Error::BufferError));
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
    /// * `StackError` - If validation of any parameters fails.
    pub fn open(ssid: &'a Ssid) -> Result<Self, StackError> {
        Ok(Self {
            ssid,
            key: Credentials::Open,
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::from_bits(PROVISIONING_DEFAULT_IP),
        })
    }

    #[cfg(feature = "wep")]
    /// Creates configuration for a WEP-secured access point.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    /// * `key` - The WEP security key, either 10 bytes (for 40-bit) or 26 bytes (for 104-bit).
    /// * `key_index` - Wep Key Index; typically between 1 and 4.
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - The configured `AccessPoint` with WEP security on success.
    /// * `StackError` - If parameter validation fails.
    pub fn wep(ssid: &'a Ssid, key: &'a WepKey, key_index: u8) -> Result<Self, StackError> {
        Ok(Self {
            ssid,
            key: Credentials::Wep(key.clone(), key_index),
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::from_bits(PROVISIONING_DEFAULT_IP),
        })
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
    /// * `StackError` - If parameter validation fails.
    pub fn wpa(ssid: &'a Ssid, key: &'a WpaKey) -> Result<Self, StackError> {
        Ok(Self {
            ssid,
            key: Credentials::WpaPSK(*key),
            channel: WifiChannel::Channel1,
            ssid_hidden: false,
            ip: Ipv4Addr::from_bits(PROVISIONING_DEFAULT_IP),
        })
    }

    /// Sets the static IP address for the configured access point.
    ///
    /// # Arguments
    ///
    /// * `ip` - The new static IPv4 address to assign to the access point.
    ///
    /// # Warning
    ///
    /// Due to a WINC firmware limitation, the client's IP address is always assigned as `x.x.x.100`.
    ///
    /// # Returns
    ///
    /// * `()` - If the IP address is successfully set.
    /// * `StackError` - If the IP address is invalid.
    pub fn set_ip(&mut self, ip: Ipv4Addr) -> Result<(), StackError> {
        let octets = ip.octets();
        // WINC fimrware limitation; IP address of client is always x.x.x.100
        if !((1..100).contains(&octets[3])) {
            return Err(StackError::WincWifiFail(Error::BufferError));
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
}
