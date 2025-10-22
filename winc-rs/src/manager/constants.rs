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

/// Maxmimum length of SSID.
pub const MAX_SSID_LEN: usize = 32;
/// Length for 104 bit string passpharase.
pub(crate) const MAX_WEP_KEY_LEN: usize = 26;
#[cfg(feature = "wep")]
/// Length for 40 bit string passpharase.
pub(crate) const MIN_WEP_KEY_LEN: usize = 10;
/// Maximum length for the WPA PSK Key.
pub const MAX_PSK_KEY_LEN: usize = 63;
/// Minimum length for the WPA PSK Key.
pub(crate) const MIN_PSK_KEY_LEN: usize = 8;
/// Maximum length for device domain name / hostname (DNS, provisioning, etc.).
pub const MAX_HOST_NAME_LEN: usize = 63;
/// Packet size of the Start Provisioning Mode request.
pub(crate) const START_PROVISION_PACKET_SIZE: usize = 204;
/// Packet size of Provisioning Info.
pub(crate) const PROVISIONING_INFO_PACKET_SIZE: usize = 100;
/// Maximum password length for the enterprise mode.
pub const MAX_S802_PASSWORD_LEN: usize = 40;
/// Maximum username length for the Enterprise mode.
pub const MAX_S802_USERNAME_LEN: usize = 20;
/// Packet size of connect request.
pub(crate) const CONNECT_AP_PACKET_SIZE: usize = 108;
/// Packet size of enable access point request.
pub(crate) const ENABLE_AP_PACKET_SIZE: usize = 136;
/// Packet size to set socket option request.
pub(crate) const SET_SOCK_OPTS_PACKET_SIZE: usize = 8;
/// Packet size to set SSL socket option request.
#[cfg(feature = "ssl")]
pub(crate) const SET_SSL_SOCK_OPTS_PACKET_SIZE: usize = 72;
/// Maximum buffer size for the TCP stack. Must be able to handle the full MTU from the chip (1440+ bytes observed).
pub(crate) const SOCKET_BUFFER_MAX_LENGTH: usize = 1500;
/// Packet Size of get random bytes request.
pub(crate) const PRNG_PACKET_SIZE: usize = 8;
// Maximum length supported by the chip in one iteration.
#[cfg(feature = "large_rng")]
pub(crate) const PRNG_DATA_LENGTH: usize = 1600 - 4 - PRNG_PACKET_SIZE;
#[cfg(not(feature = "large_rng"))]
pub(crate) const PRNG_DATA_LENGTH: usize = 32;
#[cfg(feature = "flash-rw")]
/// Page size of Flash memory.
pub(crate) const FLASH_PAGE_SIZE: usize = 256;
/// Packet Size of SSL ECC request/response.
#[cfg(feature = "experimental-ecc")]
pub(crate) const SSL_ECC_REQ_PACKET_SIZE: usize = 112;
/// Packet size of cipher suite bitmap (u32).
#[cfg(feature = "ssl")]
pub(crate) const SSL_CS_MAX_PACKET_SIZE: usize = 4;

#[repr(u32)]
pub enum Regs {
    WakeClock = 0x01,
    HostToCortusComm = 0x0b,
    EnableClock = 0x0f,
    ChipReset = 0x1400,
    ChipHalt = 0x1118,
    CortusIrq = 0x20300,
    SpiConfig = 0xE824,
    ChipId = 0x1000,
    EFuseRead = 0x1014,
    NmiState = 0x108c,
    ChipRev = 0x13f4,
    NmiPinMux0 = 0x1408,
    NmiGp1 = 0x14A0,
    NmiIntrEnable = 0x1a00,
    NmiRev = 0x207ac,
    WaitForHost = 0x207bc,
    NmiGp2 = 0xC0008,
    BootRom = 0xC000C,
    WifiHostRcvCtrl0 = 0x1070,
    WifiHostRcvCtrl1 = 0x1084,
    WifiHostRcvCtrl2 = 0x1078,
    WifiHostRcvCtrl3 = 0x106c,
    WifiHostRcvCtrl4 = 0x150400,
    #[cfg(feature = "flash-rw")]
    FlashCommandCount = 0x10204,
    #[cfg(feature = "flash-rw")]
    FlashDataCount = 0x10208,
    #[cfg(feature = "flash-rw")]
    FlashBuffer1 = 0x1020c,
    #[cfg(feature = "flash-rw")]
    FlashBuffer2 = 0x10210,
    #[cfg(feature = "flash-rw")]
    FlashBufferDirectory = 0x10214,
    #[cfg(feature = "flash-rw")]
    FlashTransferDone = 0x10218,
    #[cfg(feature = "flash-rw")]
    FlashDmaAddress = 0x1021c,
    #[cfg(feature = "flash-rw")]
    FlashMsbControl = 0x10220,
    #[cfg(feature = "flash-rw")]
    FlashTransmitControl = 0x10224,
    #[cfg(feature = "flash-rw")]
    FlashSharedMemory = 0xd0000,
    #[cfg(feature = "flash-rw")]
    FlashPinMux = 0x1410,
}

impl From<Regs> for u32 {
    fn from(val: Regs) -> Self {
        val as u32
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq)]
pub enum WifiConnError {
    NoError,
    ScanFail,
    JoinFail,
    AuthFail,
    AssocFail,
    ConnInProgress,
    ConnListEmpty,
    Unhandled,
}

impl From<u8> for WifiConnError {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::NoError,
            1 => Self::ScanFail,
            2 => Self::JoinFail,
            3 => Self::AuthFail,
            4 => Self::AssocFail,
            5 => Self::ConnInProgress,
            /* Error codes for default connection response */
            232 => Self::ConnInProgress,
            233 => Self::JoinFail,
            234 => Self::ScanFail,
            235 => Self::ConnListEmpty,
            _ => Self::Unhandled,
        }
    }
}

impl core::fmt::Display for WifiConnError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Type of authentication used by an access point
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum AuthType {
    #[default]
    Invalid,
    Open,
    WpaPSK,
    WEP,
    S802_1X,
}

impl From<u8> for AuthType {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Open,
            2 => Self::WpaPSK,
            3 => Self::WEP,
            4 => Self::S802_1X,
            _ => Self::Invalid,
        }
    }
}

impl From<AuthType> for u8 {
    fn from(val: AuthType) -> Self {
        val as Self
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default)]
pub enum WifiConnState {
    #[default]
    Unhandled,
    Disconnected,
    Connected,
}

impl From<u8> for WifiConnState {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::Disconnected,
            1 => Self::Connected,
            _ => Self::Unhandled,
        }
    }
}

impl core::fmt::Display for WifiConnState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Wifi Request IDs.
#[derive(Copy, Clone)]
pub enum WifiRequest {
    Restart = 0x01,            // implemented
    SetMacAddress = 0x02,      // M2M_WIFI_REQ_SET_MAC_ADDRESS
    CurrentRssi = 0x03,        // implemented
    GetConnInfo = 0x05,        // implemented
    SetDeviceName = 0x07, // M2M_WIFI_REQ_SET_DEVICE_NAME + null-terminated string up to 48 bytes
    StartProvisionMode = 0x08, // M2M_WIFI_REQ_START_PROVISION_MODE + tstrM2MProvisionModeConfig
    StopProvisionMode = 0x0A, // M2M_WIFI_REQ_STOP_PROVISION_MODE + no params
    SetSysTime = 0x0B,    // M2M_WIFI_REQ_SET_SYS_TIME + uint32 UTC seconds
    EnableSntpClient = 0x0C, // M2M_WIFI_REQ_ENABLE_SNTP_CLIENT + no params
    DisableSntpClient = 0x0D, // M2M_WIFI_REQ_DISABLE_SNTP_CLIENT + no params
    CustInfoElement = 0x0F, // M2M_WIFI_REQ_CUST_INFO_ELEMENT + up to 252 bytes
    Scan = 0x10,          // implemented
    ScanResult = 0x12,    // implemented
    SetScanOption = 0x14, // M2M_WIFI_REQ_SET_SCAN_OPTION + tstrM2MScanOption
    SetScanRegion = 0x15, // M2M_WIFI_REQ_SET_SCAN_REGION + uint16 region code
    SetPowerProfile = 0x16, // M2M_WIFI_REQ_SET_POWER_PROFILE + uint8 power profile
    SetTxPower = 0x17,    // M2M_WIFI_REQ_SET_TX_POWER + uint8 tx power level
    SetBatteryVoltage = 0x18, // M2M_WIFI_REQ_SET_BATTERY_VOLTAGE + uint16 voltage in millivolts
    SetEnableLogs = 0x19, // M2M_WIFI_REQ_SET_ENABLE_LOGS + uint8 enable/disable
    GetSysTime = 0x1A,    // M2M_WIFI_REQ_GET_SYS_TIME + no params
    SendEthernetPacket = 0x1C, // M2M_WIFI_REQ_SEND_ETHERNET_PACKET + tstrM2MWifiTxPacketInfo + data
    SetMacMcast = 0x1E,   // M2M_WIFI_REQ_SET_MAC_MCAST + mac bytes + uint8 enable/disable
    GetPrng = 0x1F,       // M2M_WIFI_REQ_GET_PRNG + tstrPrng
    ScanSsidList = 0x21,  // M2M_WIFI_REQ_SCAN_SSID_LIST + tstrM2MScan + SSID list (count + strings)
    SetGains = 0x22,      // M2M_WIFI_REQ_SET_GAINS + tstrM2mWifiGainsParams
    PassiveScan = 0x23,   // M2M_WIFI_REQ_PASSIVE_SCAN + tstrM2MScan
    // sta mode commands
    Connect = 0x28,        // implemented
    DefaultConnect = 0x29, // implemented
    Disconnect = 0x2B,     // M2M_WIFI_REQ_DISCONNECT + no params
    Sleep = 0x2D,          // M2M_WIFI_REQ_SLEEP + tstrM2mPsType power save conf
    WpsScan = 0x2E,        // M2M_WIFI_REQ_WPS_SCAN + not available / documented
    Wps = 0x2F,            // M2M_WIFI_REQ_WPS + tstrM2MWPSConnect
    DisableWps = 0x31,     // M2M_WIFI_REQ_DISABLE_WPS + no params
    // DhcpConf = 0x32, // M2M_WIFI_REQ_DHCP_CONF < this is wrongly named as REQ in original code
    EnableMonitoring = 0x35, // M2M_WIFI_REQ_ENABLE_MONITORING + tstrM2MWifiMonitorModeCtrl
    DisableMonitoring = 0x36, // M2M_WIFI_REQ_DISABLE_MONITORING + no params
    SendWifiPacket = 0x38,   // M2M_WIFI_REQ_SEND_WIFI_PACKET + tstrM2MWifiTxPacketInfo + data
    LsnInt = 0x39,           // M2M_WIFI_REQ_LSN_INT + tstrM2mLsnInt
    Doze = 0x3A,             // M2M_WIFI_REQ_DOZE + tstrM2mSlpReqTime
    EnableAp = 0x46,         // M2M_WIFI_REQ_ENABLE_AP + tstrM2MAPConfig
    DisableAp = 0x47,        // M2M_WIFI_REQ_DISABLE_AP
}

impl From<WifiRequest> for u8 {
    fn from(val: WifiRequest) -> Self {
        val as Self
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default)]
#[rustfmt::skip]  // Because of the commented out responses
pub enum WifiResponse {
    #[default]
    Unhandled,
    CurrentRssi,      // Done
    ConnInfo,         // Done
    ProvisionInfo,    // M2M_WIFI_RESP_PROVISION_INFO + tstrM2MProvisionInfo
    ScanDone,         // Done
    ScanResult,       // Done
    GetSysTime,       // Done
    GetPrng,          // M2M_WIFI_RESP_GET_PRNG + tstrPrng
    DefaultConnect,   // Done
    DhcpConf,         // Done
    ConStateChanged,  // Done
    IpConflict,       // Done
    ClientInfo,       // M2M_WIFI_RESP_CLIENT_INFO + 4-byte buffer
    Wps,              // M2M_WIFI_REQ_WPS + tstrM2MWPSInfo
    EthernetRxPacket, // M2M_WIFI_RESP_ETHERNET_RX_PACKET + tstrM2mIpRsvdPkt + data
    WifiRxPacket,     // M2M_WIFI_RESP_WIFI_RX_PACKET + tstrM2MWifiRxPacketInfo + data
    // MemoryRecover,   // M2M_WIFI_RESP_MEMORY_RECOVER + 4-byte buffer (commented out in code)
    // IpConfigured,    // M2M_WIFI_RESP_IP_CONFIGURED + no specific data (internal use)
/* No Crypto for now
    CryptoSha256Init,   // M2M_CRYPTO_RESP_SHA256_INIT + tstrCyptoResp (crypto mode)
    CryptoSha256Update, // M2M_CRYPTO_RESP_SHA256_UPDATE + tstrCyptoResp (crypto mode)
    CryptoSha256Finish, // M2M_CRYPTO_RESP_SHA256_FINSIH + tstrCyptoResp (crypto mode, typo in original)
    CryptoRsaSignGen,   // M2M_CRYPTO_RESP_RSA_SIGN_GEN + tstrCyptoResp (crypto mode)
    CryptoRsaSignVerify,// M2M_CRYPTO_RESP_RSA_SIGN_VERIFY + tstrCyptoResp (crypto mode)
*/
}

#[rustfmt::skip] // Because of the commented out responses
impl From<u8> for WifiResponse {
    fn from(v: u8) -> Self {
        match v {
            0x04 => Self::CurrentRssi,      // M2M_WIFI_RESP_CURRENT_RSSI
            0x06 => Self::ConnInfo,         // M2M_WIFI_RESP_CONN_INFO
            0x09 => Self::ProvisionInfo,    // M2M_WIFI_RESP_PROVISION_INFO
            0x11 => Self::ScanDone,         // M2M_WIFI_RESP_SCAN_DONE
            0x13 => Self::ScanResult,       // M2M_WIFI_RESP_SCAN_RESULT
            0x1B => Self::GetSysTime,       // M2M_WIFI_RESP_GET_SYS_TIME
            0x20 => Self::GetPrng,          // M2M_WIFI_RESP_GET_PRNG
            0x2A => Self::DefaultConnect,   // M2M_WIFI_RESP_DEFAULT_CONNECT
            0x2C => Self::ConStateChanged,  // M2M_WIFI_RESP_CON_STATE_CHANGED
            0x32 => Self::DhcpConf,         // M2M_WIFI_REQ_DHCP_CONF (misnamed)
            0x34 => Self::IpConflict,       // M2M_WIFI_RESP_IP_CONFLICT
            0x2F => Self::Wps,              // M2M_WIFI_REQ_WPS (response)
            0x65 => Self::ClientInfo,       // M2M_WIFI_RESP_CLIENT_INFO
            0x1D => Self::EthernetRxPacket, // M2M_WIFI_RESP_ETHERNET_RX_PACKET
            0x37 => Self::WifiRxPacket,     // M2M_WIFI_RESP_WIFI_RX_PACKET
            // 0x0E => Self::MemoryRecover,   // M2M_WIFI_RESP_MEMORY_RECOVER (commented out)
            // 0x33 => Self::IpConfigured,       // M2M_WIFI_RESP_IP_CONFIGURED
/* No Crypto for now
            0x02 => Self::CryptoSha256Init,   // M2M_CRYPTO_RESP_SHA256_INIT
            //0x04 =>Self::CryptoSha256Update,// M2M_CRYPTO_RESP_SHA256_UPDATE ( overlaps with CurrentRssi)
            //0x06 =>Self::CryptoSha256Finish,// M2M_CRYPTO_RESP_SHA256_FINSIH ( overlaps with ConnInfo)
            0x08 => Self::CryptoRsaSignGen,   // M2M_CRYPTO_RESP_RSA_SIGN_GEN
            0x0A => Self::CryptoRsaSignVerify,// M2M_CRYPTO_RESP_RSA_SIGN_VERIFY
*/
            _ => Self::Unhandled,
        }
    }
}
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum IpCode {
    #[default]
    Unhandled,
    // Implemented Socket Commands
    Bind = 0x41,       // SOCKET_CMD_BIND + tstrBindCmd (exists, works)
    Listen = 0x42,     // SOCKET_CMD_LISTEN + tstrListenCmd (exists, works)
    Accept = 0x43,     // SOCKET_CMD_ACCEPT + no params (exists, works)
    Connect = 0x44,    // SOCKET_CMD_CONNECT + tstrConnectCmd (exists, works)
    Send = 0x45,       // SOCKET_CMD_SEND + tstrSendCmd + data (exists, works)
    Recv = 0x46,       // SOCKET_CMD_RECV + tstrRecvCmd (exists, works)
    SendTo = 0x47,     // SOCKET_CMD_SENDTO + tstrSendCmd + data (works)
    RecvFrom = 0x48,   // SOCKET_CMD_RECVFROM + tstrRecvCmd (exists, works)
    Close = 0x49,      // SOCKET_CMD_CLOSE + tstrCloseCmd (exists, works)
    DnsResolve = 0x4A, // SOCKET_CMD_DNS_RESOLVE + hostname string (works)
    Ping = 0x52,       // SOCKET_CMD_PING + tstrPingCmd (exists, works)
    #[cfg(feature = "ssl")]
    SslConnect = 0x4B, // SOCKET_CMD_SSL_CONNECT + tstrConnectCmd
    #[cfg(feature = "ssl")]
    SslSend = 0x4C, // SOCKET_CMD_SSL_SEND + tstrSendCmd + data
    #[cfg(feature = "ssl")]
    SslRecv = 0x4D, // SOCKET_CMD_SSL_RECV + tstrRecvCmd
    #[cfg(feature = "ssl")]
    SslClose = 0x4E, // SOCKET_CMD_SSL_CLOSE + tstrCloseCmd
    SetSocketOption = 0x4F, // SOCKET_CMD_SET_SOCKET_OPTION + tstrSetSocketOptCmd
    #[cfg(feature = "ssl")]
    SslCreate = 0x50, // SOCKET_CMD_SSL_CREATE + tstrSSLSocketCreateCmd
    #[cfg(feature = "ssl")]
    SslSetSockOpt = 0x51, // SOCKET_CMD_SSL_SET_SOCK_OPT + tstrSSLSetSockOptCmd
    #[cfg(feature = "ssl")]
    SslBind = 0x54, // SOCKET_CMD_SSL_BIND + tstrBindCmd
    #[cfg(feature = "ssl")]
    SslExpCheck = 0x55, // SOCKET_CMD_SSL_EXP_CHECK + tstrSslCertExpSettings

                       // Unimplemented Socket Commands (defined but not used)
                       // Socket = 0x40,      // SOCKET_CMD_SOCKET + no params (not sent, implicit in host logic)
                       // SslSetCsList = 0x53, // SOCKET_CMD_SSL_SET_CS_LIST + no specific data
}

/// Implementation to convert `IpCode` to `u8` value.
impl From<IpCode> for u8 {
    fn from(val: IpCode) -> Self {
        val as Self
    }
}

impl From<u8> for IpCode {
    fn from(v: u8) -> Self {
        match v {
            0x41 => Self::Bind,
            0x42 => Self::Listen,
            0x43 => Self::Accept,
            0x44 => Self::Connect,
            0x45 => Self::Send,
            0x46 => Self::Recv,
            0x47 => Self::SendTo,
            0x48 => Self::RecvFrom,
            0x49 => Self::Close,
            0x4A => Self::DnsResolve,
            #[cfg(feature = "ssl")]
            0x4B => Self::SslConnect,
            #[cfg(feature = "ssl")]
            0x4C => Self::SslSend,
            #[cfg(feature = "ssl")]
            0x4D => Self::SslRecv,
            #[cfg(feature = "ssl")]
            0x4E => Self::SslClose,
            0x4F => Self::SetSocketOption,
            #[cfg(feature = "ssl")]
            0x50 => Self::SslCreate,
            #[cfg(feature = "ssl")]
            0x51 => Self::SslSetSockOpt,
            0x52 => Self::Ping,
            #[cfg(feature = "ssl")]
            0x54 => Self::SslBind,
            #[cfg(feature = "ssl")]
            0x55 => Self::SslExpCheck,
            _ => Self::Unhandled,
        }
    }
}

#[cfg(feature = "experimental-ota")]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default)]
/// OTA HiF Response ID.
pub(crate) enum OtaResponse {
    #[default]
    Unhandled,
    OtaNotifyUpdateInfo = 0x6A, // M2M_OTA_RESP_NOTIF_UPDATE_INFO + tstrOtaUpdateInfo (OTA mode)
    OtaUpdateStatus = 0x6B,     // M2M_OTA_RESP_UPDATE_STATUS + tstrOtaUpdateStatusResp (OTA mode)
}

#[cfg(feature = "experimental-ota")]
impl From<u8> for OtaResponse {
    fn from(v: u8) -> Self {
        match v {
            0x6A => Self::OtaNotifyUpdateInfo,
            0x6B => Self::OtaUpdateStatus,
            _ => Self::Unhandled,
        }
    }
}

#[cfg(feature = "experimental-ota")]
/// OTA Request/Operation Identifiers.
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub(crate) enum OtaRequest {
    SetUrl = 0x64,                    // M2M_OTA_REQ_NOTIF_SET_URL
    NotifyUpdate = 0x65,              // M2M_OTA_REQ_NOTIF_CHECK_FOR_UPDATE
    ScheduleToNotify = 0x66,          // M2M_OTA_REQ_NOTIF_SCHED
    StartFirmwareUpdate = 0x67,       // M2M_OTA_REQ_START_FW_UPDATE
    SwitchFirmware = 0x68,            // M2M_OTA_REQ_SWITCH_FIRMWARE
    RollbackFirmware = 0x69,          // M2M_OTA_REQ_ROLLBACK_FW
    RequestTest = 0x6C,               // M2M_OTA_REQ_TEST
    StartCortusFirmwareUpdate = 0x6D, // M2M_OTA_REQ_START_CRT_UPDATE
    SwitchCortusFirmware = 0x6E,      // M2M_OTA_REQ_SWITCH_CRT_IMG
    RollbackCortusFirmware = 0x6F,    // M2M_OTA_REQ_ROLLBACK_CRT
    Abort = 0x70,                     // M2M_OTA_REQ_ABORT
}

#[cfg(feature = "experimental-ota")]
/// Implementation to convert `OtaRequest` to `u8` value.
impl From<OtaRequest> for u8 {
    fn from(val: OtaRequest) -> Self {
        val as u8
    }
}

#[cfg(feature = "experimental-ota")]
/// OTA Update Error Codes.
#[repr(u8)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OtaUpdateError {
    NoError = 0,
    GenericFail = 1,
    InvalidArguments = 2,
    InvalidRollbackImage = 3,
    InvalidFlashSize = 4,
    AlreadyEnabled = 5,
    UpdateInProgress = 6,
    ImageVerificationFailed = 7,
    ConnectionError = 8,
    ServerError = 9,
    Aborted = 10,
    Unhandled = 0xff,
}

#[cfg(feature = "experimental-ota")]
/// Implementation to convert `u8` value to `OtaUpdateError`.
impl From<u8> for OtaUpdateError {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::NoError,
            1 => Self::GenericFail,
            2 => Self::InvalidArguments,
            3 => Self::InvalidRollbackImage,
            4 => Self::InvalidFlashSize,
            5 => Self::AlreadyEnabled,
            6 => Self::UpdateInProgress,
            7 => Self::ImageVerificationFailed,
            8 => Self::ConnectionError,
            9 => Self::ServerError,
            10 => Self::Aborted,
            _ => Self::Unhandled,
        }
    }
}

#[cfg(feature = "experimental-ota")]
#[repr(u8)]
/// OTA Update Status.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum OtaUpdateStatus {
    Download = 1,
    SwitchingFirmware = 2,
    Rollback = 3,
    Abort = 4,
    Unhandled = 0xff,
}

#[cfg(feature = "experimental-ota")]
/// Implementation to convert `u8` value to `OtaUpdateStatus`.
impl From<u8> for OtaUpdateStatus {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::Download,
            2 => Self::SwitchingFirmware,
            3 => Self::Rollback,
            4 => Self::Abort,
            _ => Self::Unhandled,
        }
    }
}

/// SSL requests
#[cfg(feature = "ssl")]
#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum SslRequest {
    Unhandled = 0xff,       // Invalid Request
    SendEccResponse = 0x02, // Send ECC Response
    NotifyCrl = 0x03,       // Update Certificate Revocation List
    SendCertificate = 0x04, // Send SSL Certificates
    SetCipherSuites = 0x05, // Set the custom cipher suites list
}

/// SSL responses
#[cfg(feature = "ssl")]
#[repr(u8)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum SslResponse {
    EccReqUpdate = 0x01,      // Response of ECC command.
    CipherSuiteUpdate = 0x06, // Response of requested changes in Cipher Suites.
    Unhandled = 0xff,         // Invalid response received.
}

/// Convert `SslRequest` to `u8` value.
#[cfg(feature = "ssl")]
impl From<SslRequest> for u8 {
    fn from(val: SslRequest) -> Self {
        val as u8
    }
}

/// Convert `SslResponse` to `u8` value.
#[cfg(feature = "ssl")]
impl From<SslResponse> for u8 {
    fn from(val: SslResponse) -> Self {
        val as u8
    }
}

/// Convert `u8` to `SslResponse` value.
#[cfg(feature = "ssl")]
impl From<u8> for SslResponse {
    fn from(val: u8) -> Self {
        match val {
            0x01 => SslResponse::EccReqUpdate,
            0x06 => SslResponse::CipherSuiteUpdate,
            _ => SslResponse::Unhandled,
        }
    }
}

/// Convert `SslRequest` to `u8` value.
#[cfg(feature = "ssl")]
impl From<u8> for SslRequest {
    fn from(val: u8) -> Self {
        match val {
            0x02 => SslRequest::SendEccResponse,
            0x03 => SslRequest::NotifyCrl,
            0x04 => SslRequest::SendCertificate,
            0x05 => SslRequest::SetCipherSuites,
            _ => SslRequest::Unhandled,
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum PingError {
    Unhandled = -1000,
    #[default]
    NoError = 0, // 0
    DestinationUnreachable = 1,
    Timeout = 2,
}

impl From<u8> for PingError {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::NoError,
            1 => Self::DestinationUnreachable,
            2 => Self::Timeout,
            _ => Self::Unhandled,
        }
    }
}

#[allow(dead_code)] // Todo: once complete maybe can remove
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum SocketError {
    Unhandled = -1000,
    #[default]
    NoError = 0, // 0
    InvalidAddress = -1,   // 255
    AddrAlreadyInUse = -2, // 254
    MaxTcpSock = -3,       // 253
    MaxUdpSock = -4,       // 252
    InvalidArg = -6,       // 250
    MaxListenSock = -7,    // 249
    Invalid = -9,          // 247
    AddrIsRequired = -11,  // 245
    ConnAborted = -12,     // 244
    Timeout = -13,         // 243
    BufferFull = -14,      // 242
}

impl From<u8> for SocketError {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::NoError,
            255 => Self::InvalidAddress,
            254 => Self::AddrAlreadyInUse,
            253 => Self::MaxTcpSock,
            252 => Self::MaxUdpSock,
            250 => Self::InvalidArg,
            249 => Self::MaxListenSock,
            247 => Self::Invalid,
            245 => Self::AddrIsRequired,
            244 => Self::ConnAborted,
            243 => Self::Timeout,
            242 => Self::BufferFull,
            _ => Self::Unhandled,
        }
    }
}

impl core::fmt::Display for SocketError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            Self::Unhandled => "Unhandled socket error",
            Self::NoError => "No error",
            Self::InvalidAddress => "Invalid address",
            Self::AddrAlreadyInUse => "Address already in use",
            Self::MaxTcpSock => "Maximum TCP sockets reached",
            Self::MaxUdpSock => "Maximum UDP sockets reached",
            Self::InvalidArg => "Invalid argument",
            Self::MaxListenSock => "Maximum listen sockets reached",
            Self::Invalid => "Invalid socket",
            Self::AddrIsRequired => "Address is required",
            Self::ConnAborted => "Connection aborted",
            Self::Timeout => "Socket timeout",
            Self::BufferFull => "Buffer full",
        };
        f.write_str(msg)
    }
}

/// Wi-Fi RF Channels
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WifiChannel {
    Channel1 = 1, // Default Value
    Channel2 = 2,
    Channel3 = 3,
    Channel4 = 4,
    Channel5 = 5,
    Channel6 = 6,
    Channel7 = 7,
    Channel8 = 8,
    Channel9 = 9,
    Channel10 = 10,
    Channel11 = 11,
    Channel12 = 12,
    Channel13 = 13,
    Channel14 = 14,
    ChannelAll = 255,
}

impl From<u8> for WifiChannel {
    fn from(n: u8) -> Self {
        match n {
            1 => WifiChannel::Channel1,
            2 => WifiChannel::Channel2,
            3 => WifiChannel::Channel3,
            4 => WifiChannel::Channel4,
            5 => WifiChannel::Channel5,
            6 => WifiChannel::Channel6,
            7 => WifiChannel::Channel7,
            8 => WifiChannel::Channel8,
            9 => WifiChannel::Channel9,
            10 => WifiChannel::Channel10,
            11 => WifiChannel::Channel11,
            12 => WifiChannel::Channel12,
            13 => WifiChannel::Channel13,
            14 => WifiChannel::Channel14,
            255 => WifiChannel::ChannelAll,
            _ => WifiChannel::Channel1, // Default Value
        }
    }
}

impl From<WifiChannel> for u8 {
    fn from(val: WifiChannel) -> Self {
        val as Self
    }
}

/// Wep Key Index
#[cfg(feature = "wep")]
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WepKeyIndex {
    NoKey = 0,
    Key1 = 1,
    Key2 = 2,
    Key3 = 3,
    Key4 = 4,
}

#[cfg(feature = "wep")]
impl From<WepKeyIndex> for u8 {
    fn from(val: WepKeyIndex) -> Self {
        val as Self
    }
}

#[cfg(feature = "wep")]
impl From<u8> for WepKeyIndex {
    fn from(n: u8) -> Self {
        match n {
            0 => WepKeyIndex::NoKey,
            1 => WepKeyIndex::Key1,
            2 => WepKeyIndex::Key2,
            3 => WepKeyIndex::Key3,
            4 => WepKeyIndex::Key4,
            _ => WepKeyIndex::NoKey, // Default Value
        }
    }
}

/// Options to configure SSL Certificate Expiry.
#[cfg(feature = "ssl")]
#[repr(u32)]
pub enum SslCertExpiryOpt {
    /// Ignore certificate expiration date validation.
    Disabled = 0x00,
    /// Validate certificate expiration date.
    /// If expired or system time is not configured, the SSL connection fails.
    Enabled = 0x01,
    /// Validate the certificate expiration date only if there is a configured system time.
    /// If there is no configured system time, the certificate expiration is bypassed and the
    /// SSL connection succeeds.
    EnabledIfSysTime = 0x02,
}

/// Converts the `SslCertExpiryOpt` value to `u32` value.
#[cfg(feature = "ssl")]
impl From<SslCertExpiryOpt> for u32 {
    fn from(opt: SslCertExpiryOpt) -> Self {
        opt as Self
    }
}

/// ECC request type.
#[cfg(feature = "experimental-ecc")]
#[repr(u16)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EccRequestType {
    #[default]
    None = 0,
    ClientEcdh = 1,
    ServerEcdh = 2,
    GenerateKey = 3,
    GenerateSignature = 4,
    VerifySignature = 5,
    Unknown,
}

/// Convert the `EccRequestType` to `u16` value.
#[cfg(feature = "experimental-ecc")]
impl From<EccRequestType> for u16 {
    fn from(val: EccRequestType) -> Self {
        val as u16
    }
}

/// Convert the `u16` value to `EccRequestType`.
#[cfg(feature = "experimental-ecc")]
impl From<u16> for EccRequestType {
    fn from(val: u16) -> Self {
        match val {
            0 => Self::None,
            1 => Self::ClientEcdh,
            2 => Self::ServerEcdh,
            3 => Self::GenerateKey,
            4 => Self::GenerateSignature,
            5 => Self::VerifySignature,
            _ => Self::Unknown,
        }
    }
}

/// ECC Curve type
#[cfg(feature = "experimental-ecc")]
#[repr(u16)]
#[derive(Debug, PartialEq, Default)]
pub enum EccCurveType {
    /// NIST-P192
    Secp192r1 = 19,
    /// NIST-P256
    Secp256r1 = 23,
    /// NIST-P384
    Secp384r1 = 24,
    /// NIST-P521
    Secp521r1 = 25,
    /// Unknown
    #[default]
    Unknown,
}

/// Convert `EccCurveType` to `u16` value.
#[cfg(feature = "experimental-ecc")]
impl From<EccCurveType> for u16 {
    fn from(val: EccCurveType) -> Self {
        val as u16
    }
}

/// Convert `u16` to `EccCurveType` value.
#[cfg(feature = "experimental-ecc")]
impl From<u16> for EccCurveType {
    fn from(val: u16) -> Self {
        match val {
            19 => Self::Secp192r1,
            23 => Self::Secp256r1,
            24 => Self::Secp384r1,
            25 => Self::Secp521r1,
            _ => Self::Unknown,
        }
    }
}

/// SSL Cipher Suites
/// By default, the WINC1500 hardware accelerator only supports AES-128.
/// To use AES-256 cipher suites, call the `ssl_set_cipher_suite` function.
#[repr(u32)]
pub enum SslCipherSuite {
    // Individual Ciphers
    RsaWithAes128CbcSha = 0x01,
    RsaWithAes128CbcSha256 = 0x02,
    DheRsaWithAes128CbcSha = 0x04,
    DheRsaWithAes128CbcSha256 = 0x08,
    RsaWithAes128GcmSha256 = 0x10,
    DheRsaWithAes128GcmSha256 = 0x20,
    RsaWithAes256CbcSha = 0x40,
    RsaWithAes256CbcSha256 = 0x80,
    DheRsaWithAes256CbcSha = 0x100,
    DheRsaWithAes256CbcSha256 = 0x200,
    #[cfg(feature = "experimental-ecc")]
    EcdheRsaWithAes128CbcSha = 0x400,
    #[cfg(feature = "experimental-ecc")]
    EcdheRsaWithAes256CbcSha = 0x800,
    #[cfg(feature = "experimental-ecc")]
    EcdheRsaWithAes128CbcSha256 = 0x1000,
    #[cfg(feature = "experimental-ecc")]
    EcdheEcdsaWithAes128CbcSha256 = 0x2000,
    #[cfg(feature = "experimental-ecc")]
    EcdheRsaWithAes128GcmSha256 = 0x4000,
    #[cfg(feature = "experimental-ecc")]
    EcdheEcdsaWithAes128GcmSha256 = 0x8000,
    // Grouped Ciphers
    /// ECC ciphers using ECC authentication with AES 128 encryption only.
    /// By default, this group is disabled on startup.
    #[cfg(feature = "experimental-ecc")]
    EccOnlyAes128 = 0xA000,
    /// ECC ciphers using any authentication with AES-128 encryption.
    /// By default, this group is disabled on startup.
    #[cfg(feature = "experimental-ecc")]
    EccAllAes128 = 0xF400,
    /// All none ECC ciphers using AES-128 encryption.
    /// By default, this group is active on startup.
    NoEccAes128 = 0x3F,
    /// All none ECC ciphers using AES-256 encryption.
    NoEccAes256 = 0x3C0,
    /// All RSA+AES (128 and 256) ciphers
    AllRsaAes = 0xD3,
    /// All supported ciphers.
    /// By default, this group is disabled on startup.
    AllCiphers = 0xFFFF,
}

/// Implementation to convert `SslCipherSuite` to `u32`.
impl From<SslCipherSuite> for u32 {
    fn from(val: SslCipherSuite) -> Self {
        val as u32
    }
}
