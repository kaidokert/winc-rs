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

pub enum Regs {
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
}

impl From<Regs> for u32 {
    fn from(val: Regs) -> Self {
        val as u32
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq)]
pub enum WifiConnError {
    Unhandled,
    ScanFail,
    JoinFail,
    AuthFail,
    AssocFail,
    ConnInProgress,
}

impl From<u8> for WifiConnError {
    fn from(val: u8) -> Self {
        match val {
            1 => Self::ScanFail,
            2 => Self::JoinFail,
            3 => Self::AuthFail,
            4 => Self::AssocFail,
            5 => Self::ConnInProgress,
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
#[derive(Debug, PartialEq, Default)]
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

#[allow(dead_code)] // Todo: once complete maybe can remove
pub enum WifiRequest {
    Restart = 0x01,
    SetMacAddress = 0x02,
    CurrentRssi = 0x03,
    GetConnInfo = 0x05,
    SetDeviceName = 0x07,
    StartProvisionMode = 0x08,
    StopProvisionMode = 0x0A,
    SetSysTime = 0x0B,
    EnableSntpClient = 0x0C,
    DisableSntpClient = 0x0D,
    CustInfoElement = 0x0F,
    Scan = 0x10,
    ScanResult = 0x12,
    SetScanOption = 0x14,
    SetScanRegion = 0x15,
    SetPowerProfile = 0x16,
    SetTxPower = 0x17,
    SetBatteryVoltage = 0x18,
    SetEnableLogs = 0x19,
    GetSysTime = 0x1A,
    SendEthernetPacket = 0x1C,
    SetMacMcast = 0x1E,
    GetPrng = 0x1F,
    ScanSsidList = 0x21,
    SetGains = 0x22,
    PassiveScan = 0x23,
    // sta mode commands
    Connect = 0x28,
    DefaultConnect = 0x29,
    Disconnect = 0x2B,
    Sleep = 0x2D,
    WpsScan = 0x2E,
    Wps = 0x2F,
    DisableWps = 0x31,
    DhcpConf = 0x32,
    EnableMonitoring = 0x35,
    DisableMonitoring = 0x36,
    SendWifiPacket = 0x38,
    LsnInt = 0x39,
    Doze = 0x3A,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default)]
pub enum WifiResponse {
    #[default]
    Unhandled,
    CurrrentRssi,    // done
    ConnInfo,        // done
    ProvisionInfo,   // todo: ( prov mode )
    ScanDone,        // Done
    ScanResult,      // done
    GetSysTime,      // done
    GetPrng,         // todo:
    DefaultConnect,  // done
    DhcpConf,        // done
    ConStateChanged, // done
    IpConflict,      // done
    ClientInfo,      // todo ( ps mode )
}

impl From<u8> for WifiResponse {
    fn from(v: u8) -> Self {
        match v {
            0x04 => Self::CurrrentRssi,
            0x06 => Self::ConnInfo,
            0x09 => Self::ProvisionInfo,
            0x11 => Self::ScanDone,
            0x13 => Self::ScanResult,
            0x1B => Self::GetSysTime,
            0x20 => Self::GetPrng,
            0x2A => Self::DefaultConnect,
            0x2C => Self::ConStateChanged,
            0x32 => Self::DhcpConf,
            0x34 => Self::IpConflict,
            0x65 => Self::ClientInfo,
            _ => Self::Unhandled,
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum IpCode {
    #[default]
    Unhandled,
    Bind = 0x41,     // exists, maybe works
    Listen = 0x42,   // exists
    Accept = 0x43,   // exists, no-op ?
    Connect = 0x44,  // exists
    Send = 0x45,     // exists
    Recv = 0x46,     // exists
    SendTo = 0x47,   // works
    RecvFrom = 0x48, // exists
    Close = 0x49,
    DnsResolve = 0x4A, // works
    Ping = 0x52,       // works!
    SetSocketOption = 0x4F,
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
            0x4F => Self::SetSocketOption,
            0x52 => Self::Ping,
            _ => Self::Unhandled,
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
