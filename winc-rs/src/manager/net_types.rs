use arrayvec::ArrayString;

use super::constants::{AuthType, MAX_PSK_KEY_LEN, MAX_SSID_LEN};
use core::net::Ipv4Addr;

/// Structure for Access Point Configuration.
pub struct AccessPoint<'a> {
    /// The SSID (name) of the access point.
    pub ssid: &'a str,
    /// The WPA/WPA2 or WEP key for the access point.
    pub key: &'a str,
    /// The channel number (1..14) used by the access point.
    pub channel: u8,
    /// The authentication type (e.g., WPA, WPA2, WEP)
    pub auth: AuthType,
    /// Whether the SSID is hidden (true for hidden).
    pub ssid_hidden: bool,
    /// IP address for access point.
    pub ip: core::net::Ipv4Addr,
}

/// Structure for Wi-Fi Credentials.
pub struct WifiCredentials {
    /// The SSID (network name) of the network.
    pub ssid: ArrayString<MAX_SSID_LEN>,
    /// The passphrase (Wi-Fi key) for the network's security.
    pub passphrase: ArrayString<MAX_PSK_KEY_LEN>,
    /// The authentication type (e.g., WPA, WPA2, etc.) used by the network.
    pub auth: AuthType,
}

impl<'a> AccessPoint<'a> {
    /// Creates configuration for an open (no security) access point.
    pub fn open(ssid: &'a str) -> Self {
        Self {
            ssid,
            key: "",
            channel: 1,
            auth: AuthType::Open,
            ssid_hidden: false,
            ip: Ipv4Addr::new(192, 168, 1, 1),
        }
    }

    #[cfg(feature = "enable-wep")]
    /// Creates configuration for a WEP-secured access point.
    pub fn wep(ssid: &'a str, key: &'a str) -> Self {
        Self {
            ssid,
            key,
            channel: 1,
            auth: AuthType::WEP,
            ssid_hidden: false,
            ip: Ipv4Addr::new(192, 168, 1, 1),
        }
    }

    /// Creates configuration for a WPA or WPA2-secured access point.
    pub fn wpa(ssid: &'a str, key: &'a str) -> Self {
        Self {
            ssid,
            key,
            channel: 1,
            auth: AuthType::WpaPSK,
            ssid_hidden: false,
            ip: Ipv4Addr::new(192, 168, 1, 1),
        }
    }

    //pub fn validate_pramaters(& self) -> Result
}
