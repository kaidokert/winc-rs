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

use super::constants::{AuthType, WifiChannel, MAX_PSK_KEY_LEN, MAX_SSID_LEN, MIN_PSK_KEY_LEN};
use core::net::Ipv4Addr;

// Default IP address for provisioning mode.
const PROVISIONING_DEFAULT_IP: u32 = 0xC0A80101;

/// Structure for Wi-Fi Credentials.
pub struct WifiCredentials {
    /// The SSID (network name) of the network.
    pub ssid: [u8; MAX_SSID_LEN],
    /// The passphrase (Wi-Fi key) for the network's security.
    pub key: [u8; MAX_PSK_KEY_LEN],
    /// The authentication type (e.g., WPA, WPA2, etc.) used by the network.
    pub auth: AuthType,
}

/// Structure for Access Point Configuration.
pub struct AccessPoint {
    /// Structure for storing Wifi Credentials.
    pub credentials: WifiCredentials,
    /// The channel number (1..14) or 255 for all channels used by the access point.
    pub channel: WifiChannel,
    /// Whether the SSID is hidden (true for hidden).
    pub ssid_hidden: bool,
    /// IP address for the access point. The last octet must be in the range 1 to 100,
    /// for example: 192.168.1.1 to 192.168.1.100.
    pub ip: Ipv4Addr,
}

impl WifiCredentials {
    /// Checks whether the provided key length is valid for the given authentication type.
    ///
    /// # Arguments
    ///
    /// * `auth` - The authentication method (e.g., Open, WEP, WPA2).
    /// * `key_len` - The length of the security key in bytes.
    ///
    /// # Returns
    ///
    /// * `()` - If the key length is valid for the specified authentication type.
    /// * `StackError` - If the key length is invalid.
    fn check_key_validity(auth: AuthType, key_len: usize) -> Result<(), StackError> {
        match auth {
            AuthType::WpaPSK => {
                if !((MIN_PSK_KEY_LEN..=MAX_PSK_KEY_LEN).contains(&key_len)) {
                    return Err(StackError::WincWifiFail(Error::BufferError));
                }
            }
            #[cfg(feature = "wep")]
            AuthType::WEP => {
                if (key_len != MAX_WEP_KEY_LEN) && (key_len != MIN_WEP_KEY_LEN) {
                    return Err(StackError::WincWifiFail(Error::BufferError));
                }
            }
            _ => {
                // do nothing
            }
        }

        Ok(())
    }

    /// Creates new Wi-Fi credentials from the provided parameters.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name) as a UTF-8 string, no more than 32 bytes.
    /// * `key` - The security key as a UTF-8 string; must meet length requirements based on the `auth` type.
    /// * `auth` - The authentication method (e.g., Open, WEP, WPA2).
    ///
    /// # Notes
    ///
    /// For WPA, the security key must be at least 8 bytes (MIN) and no more than 63 bytes long.
    /// For WEP, the security key should be 10 bytes for 40-bit and 26 bytes for 104-bit.
    ///
    /// # Returns
    ///
    /// * `WifiCredentials` - The configured Wi-Fi credentials on success.
    /// * `StackError` - If any parameter fails validation.
    pub fn new(ssid: &str, key: &str, auth: AuthType) -> Result<Self, StackError> {
        // SSID
        let mut _ssid = [0u8; MAX_SSID_LEN];
        let _ssid_len = ssid.len().min(MAX_SSID_LEN);
        // Passphrase
        let mut _key = [0u8; MAX_PSK_KEY_LEN];
        let _key_len = key.len().min(MAX_PSK_KEY_LEN);
        // Check the key length validity
        Self::check_key_validity(auth, key.len())?;
        // Copy the SSID
        _ssid[.._ssid_len].copy_from_slice(&(ssid.as_bytes())[.._ssid_len]);
        // Copy the Passphrase
        _key[.._key_len].copy_from_slice(&(key.as_bytes()[.._key_len]));
        Ok(Self {
            ssid: _ssid,
            key: _key,
            auth,
        })
    }

    /// Creates new Wi-Fi credentials with open (no security) access.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name) as a UTF-8 string, no more than 32 bytes.
    ///
    /// # Returns
    ///
    /// * `WifiCredentials` - The configured Wi-Fi credentials without security.
    /// * `StackError` - If the SSID exceeds the length limit.
    pub fn open(ssid: &str) -> Result<Self, StackError> {
        Self::new(ssid, "", AuthType::Open)
    }

    /// Creates a wifi configuration for a WPA or WPA2-secured access point.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    /// * `key` - The WPA security key, up to 63 bytes (as per WPA/WPA2 specification).
    ///
    /// # Returns
    ///
    /// * `WifiCredentials` - The configured Wi-Fi credentials with WPA-PSK security on success.
    /// * `StackError` - If parameter validation fails.
    pub fn wpa(ssid: &str, key: &str) -> Result<Self, StackError> {
        Self::new(ssid, key, AuthType::WpaPSK)
    }

    #[cfg(feature = "wep")]
    /// Creates a wifi configuration for a WEP secured access point.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    /// * `key` - The WEP security key, either 10 bytes (for 40-bit) or 26 bytes (for 104-bit).
    ///
    /// # Returns
    ///
    /// * `WifiCredentials` - The configured Wi-Fi credentials with WPA-PSK security on success.
    /// * `StackError` - If parameter validation fails.
    pub fn wep(ssid: &str, key: &str) -> Result<Self, StackError> {
        Self::new(ssid, passphrase, AuthType::WEP)
    }

    /// Creates an wifi configuration from raw byte slices.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID as a byte slice; no more than 32 bytes.
    /// * `key` - The security key as a byte slice; must meet length requirements based on the `auth` type.
    /// * `auth` - The authentication method (e.g., Open, WEP, WPA2).
    ///
    /// # Returns
    ///
    /// * `WifiCredentials` - The configured WifiCredentials on success.
    /// * `StackError` - If validation of any parameters fails.
    pub fn from_bytes(ssid: &[u8], key: &[u8], auth: AuthType) -> Result<Self, StackError> {
        // SSID
        let mut _ssid = [0u8; MAX_SSID_LEN];
        let _ssid_len = ssid.len().min(MAX_SSID_LEN);
        // Passphrase
        let mut _key = [0u8; MAX_PSK_KEY_LEN];
        let _key_len = key.len().min(MAX_PSK_KEY_LEN);
        // Check the key length validity
        Self::check_key_validity(auth, key.len())?;
        // Copy the SSID
        _ssid[.._ssid_len].copy_from_slice(&ssid[.._ssid_len]);
        // Copy the Passphrase
        _key[.._key_len].copy_from_slice(&key[.._key_len]);
        Ok(Self {
            ssid: _ssid,
            key: _key,
            auth,
        })
    }
}

impl AccessPoint {
    /// Creates a new access point configuration with the provided parameters.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    /// * `key` - The security key or password, length depends on the `auth` type.
    /// * `auth` - The authentication method (e.g., Open, WPA2).
    /// * `channel` - The Wi-Fi channel to operate on (typically between 1 and 14).
    /// * `ssid_hidden` - Whether the SSID should be hidden from network scans (true for hidden).
    /// * `ip` - The static IPv4 address to assign to the access point.
    ///
    /// # Notes
    ///
    /// For WPA, the security key must be at least 8 bytes (MIN) and no more than 63 bytes long.
    /// For WEP, the security key should be 10 bytes for 40-bit and 26 bytes for 104-bit.
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - Configured access point structure on success.
    /// * `StackError` - If validation of any parameters fails.
    pub fn new(
        ssid: &str,
        key: &str,
        auth: AuthType,
        channel: WifiChannel,
        ssid_hidden: bool,
        ip: Ipv4Addr,
    ) -> Result<Self, StackError> {
        let octets = ip.octets();
        if !((1..100).contains(&octets[3])) {
            return Err(StackError::WincWifiFail(Error::BufferError));
        }
        Ok(Self {
            credentials: WifiCredentials::new(ssid, key, auth)?,
            channel,
            ssid_hidden,
            ip: Ipv4Addr::from(octets),
        })
    }

    /// Creates configuration for an open (no security) access point.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - The configured `AccessPoint` with open (no security) on success.
    /// * `StackError` - If validation of any parameters fails.
    pub fn open(ssid: &str) -> Result<Self, StackError> {
        Ok(Self {
            credentials: WifiCredentials::new(ssid, "", AuthType::Open)?,
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
    ///
    /// # Returns
    ///
    /// * `AccessPoint` - The configured `AccessPoint` with WEP security on success.
    /// * `StackError` - If parameter validation fails.
    pub fn wep(ssid: &'a str, key: &'a str) -> Result<Self, StackError> {
        Ok(Self {
            credentials: WifiCredentials::new(ssid, key, AuthType::WEP)?,
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
    pub fn wpa(ssid: &str, key: &str) -> Result<Self, StackError> {
        Ok(Self {
            credentials: WifiCredentials::new(ssid, key, AuthType::WpaPSK)?,
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
