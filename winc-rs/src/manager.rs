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

use crate::errors::Error;
use core::fmt::Debug;

use crate::socket::Socket;
use crate::transfer::Xfer;

mod chip_access;
mod constants;
mod requests;
mod responses;
use crate::{debug, info, trace};
use chip_access::ChipAccess;
use constants::WifiRequest;
pub use constants::{AuthType, PingError, SocketError, WifiConnState}; // todo response shouldn't be leaking
use constants::{IpCode, Regs, WifiResponse};
use requests::*;
pub use responses::IPConf;
use responses::*;

pub use constants::WifiConnError;
pub use responses::{ConnectionInfo, ScanResult};

use core::net::{Ipv4Addr, SocketAddrV4};

#[cfg(feature = "defmt")]
use crate::nonstd::Ipv4AddrFormatWrapper;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default)]
enum HifGroup {
    #[default]
    Unhandled,
    Wifi(WifiResponse),
    Ip(IpCode),
}

impl From<[u8; 2]> for HifGroup {
    fn from(v: [u8; 2]) -> Self {
        match v[0] {
            1 => Self::Wifi(v[1].into()),
            2 => Self::Ip(v[1].into()),
            _ => Self::Unhandled,
        }
    }
}
impl From<HifGroup> for u8 {
    fn from(v: HifGroup) -> Self {
        match v {
            HifGroup::Wifi(_) => 1,
            HifGroup::Ip(_) => 2,
            _ => 0xFF,
        }
    }
}

fn hif_header_parse(hdr: [u8; 4]) -> Result<(HifGroup, u16), Error> {
    let code: [u8; 2] = hdr[..2].try_into().unwrap();
    let len = u16::from_le_bytes(hdr[2..].try_into().unwrap());
    Ok((code.into(), len))
}

const HIF_HEADER_OFFSET: usize = 8;
const ETHERNET_HEADER_LENGTH: usize = 14;
const ETHERNET_HEADER_OFFSET: usize = 34;
const IP_PACKET_OFFSET: usize = ETHERNET_HEADER_LENGTH + ETHERNET_HEADER_OFFSET; // - HIF_HEADER_OFFSET;

pub const SOCKET_BUFFER_MAX_LENGTH: usize = 1400;

// todo this needs to be used
#[allow(dead_code)]
const TCP_SOCK_MAX: usize = 7;
#[allow(dead_code)]
const UDP_SOCK_MAX: usize = 4;
#[allow(dead_code)]
const MAX_SOCKET: usize = TCP_SOCK_MAX + UDP_SOCK_MAX;

// todo: clean up no_std / std logging delta
pub trait EventListener {
    fn on_rssi(&mut self, level: i8) {
        debug!("Got RSSI:{}", level)
    }
    fn on_resolve(&mut self, ip: Ipv4Addr, host: &str) {
        #[cfg(not(feature = "defmt"))]
        debug!("Got resolve ip:{} host:{}", ip, host);
        #[cfg(feature = "defmt")]
        debug!(
            "Got resolve ip:{} host:{}",
            Ipv4AddrFormatWrapper::new(&ip),
            host
        );
    }
    fn on_default_connect(&mut self, connected: bool) {
        debug!("Got connected {}", connected)
    }
    fn on_dhcp(&mut self, conf: IPConf) {
        debug!("IP config: {}", conf)
    }
    fn on_connstate_changed(&mut self, state: WifiConnState, err: WifiConnError) {
        debug!("Connstate changed, state:{} error:{}", state, err)
    }
    fn on_connection_info(&mut self, info: ConnectionInfo) {
        debug!("Conninfo, state:{}", info)
    }
    fn on_scan_result(&mut self, result: ScanResult) {
        debug!("Scanresult {}", result)
    }
    fn on_scan_done(&mut self, num_aps: u8, err: WifiConnError) {
        debug!("Scan done, aps:{} error:{}", num_aps, err)
    }
    fn on_system_time(&mut self, year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) {
        debug!(
            "on_system_time: {}-{}-{} {}:{}:{}",
            year, month, day, hour, minute, second
        )
    }
    fn on_ip_conflict(&mut self, ip: Ipv4Addr) {
        #[cfg(not(feature = "defmt"))]
        debug!("on_ip_conflict: {}", ip);
        #[cfg(feature = "defmt")]
        debug!("on_ip_conflict: {}", Ipv4AddrFormatWrapper::new(&ip));
    }
    fn on_ping(
        &mut self,
        ip: Ipv4Addr,
        token: u32,
        rtt: u32,
        num_successful: u16,
        num_failed: u16,
        error: PingError,
    ) {
        #[cfg(not(feature = "defmt"))]
        debug!(
            "on_ping: ip:{} token:{} rtt:{} succeeded:{} failed:{} error:{:?}",
            ip, token, rtt, num_successful, num_failed, error
        );
        #[cfg(feature = "defmt")]
        debug!(
            "on_ping: ip:{} token:{} rtt:{} succeeded:{} failed:{} error:{:?}",
            Ipv4AddrFormatWrapper::new(&ip),
            token,
            rtt,
            num_successful,
            num_failed,
            error
        );
    }
    fn on_bind(&mut self, sock: Socket, err: SocketError) {
        debug!("on_bind: sock:{:?} error:{:?}", sock, err)
    }
    fn on_listen(&mut self, sock: Socket, err: SocketError) {
        debug!("on_listen: sock:{:?} error:{:?}", sock, err)
    }
    fn on_accept(
        &mut self,
        address: SocketAddrV4,
        listen_socket: Socket,
        accepted_socket: Socket,
        data_offset: u16,
    ) {
        // todo : offset seems superflous
        #[cfg(not(feature = "defmt"))]
        debug!(
            "on_accept: sockaddr:{} listen_socket:{:?} accept_socket:{:?} data_offset:{}",
            address, listen_socket, accepted_socket, data_offset
        );
        #[cfg(feature = "defmt")]
        debug!(
            "on_accept: sockaddr ip:{} port:{} listen_socket:{:?} accept_socket:{:?} data_offset:{}",
            Ipv4AddrFormatWrapper::new(address.ip()),  address.port(),
            listen_socket, accepted_socket, data_offset
        );
    }
    fn on_connect(&mut self, socket: Socket, err: SocketError) {
        debug!("on_connect: socket:{:?} error:{:?}", socket, err)
    }
    fn on_send_to(&mut self, socket: Socket, len: i16) {
        debug!("on_send_to: socket:{:?} length:{:?}", socket, len)
    }
    fn on_send(&mut self, socket: Socket, len: i16) {
        debug!("on_send: socket:{:?} length:{:?}", socket, len)
    }
    fn on_recv(&mut self, socket: Socket, address: SocketAddrV4, data: &[u8], err: SocketError) {
        #[cfg(not(feature = "defmt"))]
        debug!(
            "on_recv: socket:{:?} address:{:?} data len:{} err:{:?}",
            socket,
            address,
            data.len(),
            err
        );
        #[cfg(feature = "defmt")]
        debug!(
            "on_recv: socket:{:?} ip:{} port:{} data len:{} err:{:?}",
            socket,
            Ipv4AddrFormatWrapper::new(address.ip()),
            address.port(),
            data.len(),
            err
        );
    }
    fn on_recvfrom(
        &mut self,
        socket: Socket,
        address: SocketAddrV4,
        data: &[u8],
        err: SocketError,
    ) {
        #[cfg(not(feature = "defmt"))]
        debug!(
            "on_recvfrom: socket:{:?} address:{:?} data len:{} err:{:?}",
            socket,
            address,
            data.len(),
            err
        );
        #[cfg(feature = "defmt")]
        debug!(
            "on_recvfrom: socket:{:?} ip:{} port:{} data len:{} err:{:?}",
            socket,
            Ipv4AddrFormatWrapper::new(address.ip()),
            address.port(),
            data.len(),
            err
        );
    }
}

pub struct StubListener {}
impl EventListener for StubListener {}

pub struct Manager<X: Xfer, E: EventListener> {
    // cached addresses
    not_a_reg_ctrl_4_dma: u32, // todo: make this dynamic/proper
    chip: ChipAccess<X>,
    pub listener: E,
}

impl<X: Xfer, E: EventListener> Manager<X, E> {
    // Todo: provide a version without listener, defaulting to as stub
    pub fn from_xfer(xfer: X, listener: E) -> Self {
        Self {
            not_a_reg_ctrl_4_dma: 0xbf0000,
            chip: ChipAccess::new(xfer),
            listener,
        }
    }

    pub fn set_crc_state(&mut self, value: bool) {
        self.chip.crc = value;
    }
    pub fn chip_id(&mut self) -> Result<u32, Error> {
        self.chip.single_reg_read(Regs::ChipId.into())
    }
    pub fn chip_rev(&mut self) -> Result<u32, Error> {
        self.chip.single_reg_read(Regs::ChipRev.into())
    }

    pub fn get_firmware_ver_short(&mut self) -> Result<(Revision, Revision), Error> {
        let res = self.chip.single_reg_read(Regs::NmiRev.into())?;
        let unpack = res.to_le_bytes();
        Ok((
            Revision {
                major: unpack[1],
                minor: unpack[0] >> 4,
                patch: unpack[0] & 0xf,
            },
            Revision {
                major: unpack[3],
                minor: unpack[2] >> 4,
                patch: unpack[2] & 0xf,
            },
        ))
    }

    pub fn chip_wake() {
        unimplemented!()
        // read HOST_CORT_COMM
        // clear bit 0 of HOST_CORT_COMM
        // read WAKE_CLK_REG
        // clear bit 1 of WAKE_CLK_REG
        // read CLOCKS_EN_REG, check for bit 2
    }
    pub fn wait_bootrom(&mut self, f: &mut dyn FnMut(u32) -> bool) -> Result<bool, Error> {
        const MAX_LOOPS: u32 = 10;

        let mut val = 0;
        loop {
            let efuse = self.chip.single_reg_read(Regs::EFuseRead.into())? & 0x80000000;
            if efuse != 0 {
                break;
            }
            if val >= MAX_LOOPS || f(val) {
                return Ok(false);
            }
            val += 1;
        }
        let mut host_wait = self.chip.single_reg_read(Regs::WaitForHost.into())? & 0x1;
        const FINISH_BOOT_ROM: u32 = 0x10add09e;
        if host_wait == 0 {
            val = 0;
            while host_wait != FINISH_BOOT_ROM {
                host_wait = self.chip.single_reg_read(Regs::BootRom.into())?;
                if val > MAX_LOOPS || f(val) {
                    return Ok(false);
                }
                val += 1;
            }
        }
        let driver_rev = 0x13521352; // todo
        self.chip
            .single_reg_write(Regs::NmiState.into(), driver_rev)?;
        self.chip_id()?;
        // write conf
        let mut conf: u32 = 0;
        conf |= 0x102; // Reserved + ENABLE_PMU bit
        self.chip.single_reg_write(Regs::NmiGp1.into(), conf)?;
        let verify = self.chip.single_reg_read(Regs::NmiGp1.into())?;
        assert_eq!(verify, conf); // todo: loop
                                  // start firmware
        const START_FIRMWARE: u32 = 0xef522f61;
        self.chip
            .single_reg_write(Regs::BootRom.into(), START_FIRMWARE)?;
        Ok(true)
    }
    pub fn wait_firmware_start(&mut self, f: &mut dyn FnMut(u32) -> bool) -> Result<bool, Error> {
        const MAX_LOOPS: u32 = 10;
        const FINISH_INIT: u32 = 0x02532636;
        let mut val = 0;
        loop {
            let reg = self.chip.single_reg_read(Regs::NmiState.into())?;
            if reg == FINISH_INIT {
                break;
            }
            if val > MAX_LOOPS || f(val) {
                return Ok(false);
            }
            val += 1;
        }
        self.chip.single_reg_write(Regs::NmiState.into(), 0)?;
        Ok(true)
    }

    // wait for both bootrom and firmware start
    pub fn wait_chip_start(&mut self, f: &mut dyn FnMut(u32) -> bool) -> Result<bool, Error> {
        match self.wait_bootrom(f) {
            Ok(true) => match self.wait_firmware_start(f) {
                Ok(true) => Ok(true),
                _ => Err(Error::FirmwareStart),
            },
            _ => Err(Error::BootRoomStart),
        }
    }

    pub fn configure_spi_packetsize(&mut self) -> Result<(), Error> {
        let mut conf = self.chip.single_reg_read(Regs::SpiConfig.into())?;
        conf &= 0xFFFFFF0F; // clear
        conf |= 0x00000050; // set to 8k packet size
        self.chip.single_reg_write(Regs::SpiConfig.into(), conf)?;
        trace!("Set spiconfig to {:x}", conf);
        Ok(())
    }

    pub fn start(&mut self, wait: &mut dyn FnMut(u32) -> bool) -> Result<(), Error> {
        info!("chip id {:x} rev:{:x}", self.chip_id()?, self.chip_rev()?);
        self.configure_spi_packetsize()?;
        if !self.wait_chip_start(wait)? {
            return Err(Error::FirmwareStart);
        };
        self.enable_interrupt_pins()?;
        Ok(())
    }

    pub fn enable_interrupt_pins(&mut self) -> Result<(), Error> {
        let mut pinmux = self.chip.single_reg_read(Regs::NmiPinMux0.into())?;
        pinmux |= 1u32 << 8;
        self.chip
            .single_reg_write(Regs::NmiPinMux0.into(), pinmux)?;
        trace!("Set pinmux to {:x}", pinmux);

        let mut int_enable = self.chip.single_reg_read(Regs::NmiIntrEnable.into())?;
        int_enable |= 1u32 << 16;
        self.chip
            .single_reg_write(Regs::NmiIntrEnable.into(), int_enable)?;
        trace!("Set int enable to {:x}", int_enable);
        Ok(())
    }

    pub fn get_firmware_ver_full(&mut self) -> Result<FirmwareInfo, Error> {
        let (_, address) = self.get_gp_regs()?;
        debug!("Got address {:#x}", address);
        let mod_address = (address & 0x0000ffff) | 0x30000;
        let mut data = [0u8; 40];
        debug!("Calculated address: {:#x}", mod_address);
        self.chip.dma_block_read(mod_address, data.as_mut_slice())?;
        Ok(data.into())
    }

    fn is_interrupt_pending(&mut self) -> Result<(bool, u32), Error> {
        let val = self.chip.single_reg_read(Regs::WifiHostRcvCtrl0.into())?;
        Ok((val & 0x1 == 0x1, val))
    }
    fn clear_interrupt_pending(&mut self, ctrlreg: u32) -> Result<(), Error> {
        let setval = ctrlreg & !1;
        self.chip
            .single_reg_write(Regs::WifiHostRcvCtrl0.into(), setval)
    }
    fn get_gp_regs(&mut self) -> Result<(u32, u32), Error> {
        let read_addr = self.chip.single_reg_read(Regs::NmiGp2.into())?;
        let mod_read_add = read_addr | 0x30000;
        let mut data = [0u8; 8];
        self.chip
            .dma_block_read(mod_read_add, data.as_mut_slice())?;
        let mut mac_efuse_mib = [0u8; 4];
        mac_efuse_mib.copy_from_slice(&data[..4]);
        let mut firmware_ota_rev = [0u8; 4];
        firmware_ota_rev.copy_from_slice(&data[4..]);
        Ok((
            u32::from_le_bytes(mac_efuse_mib),
            u32::from_le_bytes(firmware_ota_rev),
        ))
    }

    // #region read

    fn read_hif_header(&mut self, ctrlreg0: u32) -> Result<(HifGroup, u16, u32), Error> {
        let _size = (ctrlreg0 >> 2) & 0xfff;
        let address = self.chip.single_reg_read(Regs::WifiHostRcvCtrl1.into())?;
        let mut hif_header = [0u8; 4];
        let slicebuffer = hif_header.as_mut_slice();
        self.chip.dma_block_read(address, slicebuffer)?;
        let hifhdr = hif_header_parse(hif_header)?;
        Ok((hifhdr.0, hifhdr.1, address))
    }

    fn read_block(&mut self, address: u32, data: &mut [u8]) -> Result<(), Error> {
        self.chip
            .dma_block_read(address + HIF_HEADER_OFFSET as u32, data)?;
        // clear rx // set rx_done
        let reg = self.chip.single_reg_read(Regs::WifiHostRcvCtrl0.into())?;
        self.chip
            .single_reg_write(Regs::WifiHostRcvCtrl0.into(), reg | 2)
    }
    // tstrRecvReply
    fn get_recv_reply<'b, const N: usize>(
        &mut self,
        address: u32,
        max_block: &'b mut [u8; N],
    ) -> Result<(Socket, SocketAddrV4, &'b [u8], SocketError), Error> {
        let mut result = [0xff; 16];
        self.read_block(address, &mut result)?;
        let (socket, addr, status, offset) = read_recv_reply(&result)?;
        debug!("Recv reply: session: {} status:{}", socket.s, status);
        let readslice = if status > 0 {
            &mut max_block[0..(status as usize)]
        } else {
            &mut max_block[0..0]
        };
        let mut err = status as u8;
        if status > 0 {
            err = 0;
            let read_address = address + offset as u32;
            self.read_block(read_address, readslice)?;
        }
        Ok((socket, addr, readslice, err.into()))
    }
    // #endregion read

    // #region write

    /// Write region
    /// Todo: This is messy
    fn prep_for_hif_send(
        &mut self,
        gid: u8,
        op: u8,
        len: u16,
        data_packet: bool,
    ) -> Result<(), Error> {
        // write nmistate
        let mut state: u32 = 0;
        state |= gid as u32;
        if data_packet {
            state |= ((op | 0x80) as u32) << 8;
        } else {
            state |= (op as u32) << 8;
        }
        state |= (len as u32) << 16;
        self.chip.single_reg_write(Regs::NmiState.into(), state)?;
        // write rcv_ctrl2 bit 1
        self.chip
            .single_reg_write(Regs::WifiHostRcvCtrl2.into(), 2)?;
        // read back rcv_ctrl2 wait for bit 1
        let mut res = self.chip.single_reg_read(Regs::WifiHostRcvCtrl2.into())?;
        res &= 2;
        assert_eq!(res, 0); // todo: loop
                            // read dma_addr from rcv_ctrl4
        self.not_a_reg_ctrl_4_dma = self.chip.single_reg_read(Regs::WifiHostRcvCtrl4.into())?;
        trace!("Dma address: {:x}", self.not_a_reg_ctrl_4_dma);
        Ok(())
    }

    fn write_hif_header(
        &mut self,
        grp: HifGroup,
        opcode: WifiRequest,
        payload: &[u8],
        data_packet: bool,
    ) -> Result<(), Error> {
        // todo: this may depend on offsets
        let pkglen = (payload.len() + HIF_HEADER_OFFSET).to_le_bytes();
        assert_eq!(pkglen[1], 0);
        // todo: clean this up. Should just be HifGroup 2 bytes
        let opp = match grp {
            HifGroup::Wifi(_) => opcode as u8,
            HifGroup::Ip(code) => code as u8,
            _ => todo!(),
        };
        let grpval = grp.into();
        self.prep_for_hif_send(
            grpval,
            opp,
            (payload.len() + HIF_HEADER_OFFSET) as u16,
            data_packet,
        )?;

        self.chip.dma_block_write(
            self.not_a_reg_ctrl_4_dma,
            &[
                grpval, opp, pkglen[0], pkglen[1], 0x00, // unused bytes
                0x00, 0x00, 0x00,
            ],
        )
    }
    fn write_ctrl3(&mut self, addr: u32) -> Result<(), Error> {
        let val = (addr << 2) | 2;
        self.chip.single_reg_write(
            Regs::WifiHostRcvCtrl3.into(),
            // dma_addr come from ctrl4
            //reg = dma_addr << 2; reg |= NBIT1;
            val,
        )
    }
    pub fn send_default_connect(&mut self) -> Result<(), Error> {
        self.write_hif_header(
            HifGroup::Wifi(WifiResponse::Unhandled),
            WifiRequest::DefaultConnect,
            &[],
            false,
        )?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }
    // TODO: send other than WPA credentials
    pub fn send_connect(
        &mut self,
        auth: AuthType,
        ssid: &str,
        password: &str,
        channel: u16,
        dont_save_credentials: bool,
    ) -> Result<(), Error> {
        let arr = write_connect_request(auth, ssid, password, channel, dont_save_credentials)?;
        self.write_hif_header(
            HifGroup::Wifi(WifiResponse::Unhandled),
            WifiRequest::Connect,
            &arr,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &arr)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_get_current_rssi(&mut self) -> Result<(), Error> {
        self.write_hif_header(
            HifGroup::Wifi(WifiResponse::Unhandled),
            WifiRequest::CurrentRssi,
            &[],
            false,
        )?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_get_conn_info(&mut self) -> Result<(), Error> {
        self.write_hif_header(
            HifGroup::Wifi(WifiResponse::Unhandled),
            WifiRequest::GetConnInfo,
            &[],
            false,
        )?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }
    pub fn send_scan(&mut self, channel: u8, scantime: u16) -> Result<(), Error> {
        let req = write_scan_req(channel, scantime)?;
        self.write_hif_header(
            HifGroup::Wifi(WifiResponse::Unhandled),
            WifiRequest::Scan,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_get_scan_result(&mut self, index: u8) -> Result<(), Error> {
        let req = [index, 0, 0, 0];
        self.write_hif_header(
            HifGroup::Wifi(WifiResponse::Unhandled),
            WifiRequest::ScanResult,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    // #region ipsend

    pub fn send_ping_req(
        &mut self,
        dest: Ipv4Addr,
        ttl: u8,
        count: u16,
        marker: u8,
    ) -> Result<(), Error> {
        let req = write_ping_req(dest, ttl, count, marker)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::Ping),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_gethostbyname(&mut self, host: &str) -> Result<(), Error> {
        let mut buffer = [0x0u8; 64];
        let req = write_gethostbyname_req(host, &mut buffer)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::DnsResolve),
            WifiRequest::Restart,
            req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }
    pub fn send_bind(&mut self, socket: Socket, address: SocketAddrV4) -> Result<(), Error> {
        // todo: AF is useless here
        let req = write_bind_req(socket, 2, address)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::Bind),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }
    pub fn send_listen(&mut self, socket: Socket, backlog: u8) -> Result<(), Error> {
        let req = write_listen_req(socket, backlog)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::Listen),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_socket_connect(
        &mut self,
        socket: Socket,
        address: SocketAddrV4,
    ) -> Result<(), Error> {
        let req = write_connect_req(socket, 2, address, 0)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::Connect),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_sendto(
        &mut self,
        socket: Socket,
        address: SocketAddrV4,
        data: &[u8],
    ) -> Result<(), Error> {
        const UDP_IP_HEADER_LENGTH: usize = 28;
        const UDP_TX_PACKET_OFFSET: usize = IP_PACKET_OFFSET + UDP_IP_HEADER_LENGTH;
        let req = write_sendto_req(socket, 2, address, data.len())?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::SendTo),
            WifiRequest::Restart,
            &req,
            true,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.chip.dma_block_write(
            self.not_a_reg_ctrl_4_dma + UDP_TX_PACKET_OFFSET as u32,
            data,
        )?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_send(&mut self, socket: Socket, data: &[u8]) -> Result<(), Error> {
        const TCP_IP_HEADER_LENGTH: usize = 40;
        const TCP_TX_PACKET_OFFSET: usize = IP_PACKET_OFFSET + TCP_IP_HEADER_LENGTH;

        // todo: offset depends on UDP or TCP
        let req = write_sendto_req(
            socket,
            2,
            SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0),
            data.len(),
        )?;
        self.write_hif_header(HifGroup::Ip(IpCode::Send), WifiRequest::Restart, &req, true)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.chip.dma_block_write(
            self.not_a_reg_ctrl_4_dma + TCP_TX_PACKET_OFFSET as u32,
            data,
        )?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_recv(&mut self, socket: Socket, timeout: u32) -> Result<(), Error> {
        let req = write_recv_req(socket, timeout)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::Recv),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_recvfrom(&mut self, socket: Socket, timeout: u32) -> Result<(), Error> {
        let req = write_recv_req(socket, timeout)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::RecvFrom),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_close(&mut self, socket: Socket) -> Result<(), Error> {
        let req = write_close_req(socket)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::Close),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_setsockopt(
        &mut self,
        socket: Socket,
        option: u8, // todo: make this an enum
        value: u32,
    ) -> Result<(), Error> {
        let req = write_setsockopt_req(socket, option, value)?;
        self.write_hif_header(
            HifGroup::Ip(IpCode::SetSocketOption),
            WifiRequest::Restart,
            &req,
            false,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn dispatch_events(&mut self) -> Result<(), Error> {
        let mut stub = StubListener {};
        self.dispatch_events_new::<StubListener>(&mut stub)
    }
    pub fn dispatch_events_new<T: EventListener>(&mut self, listener: &mut T) -> Result<(), Error> {
        let res = self.is_interrupt_pending()?;
        if !res.0 {
            return Ok(());
        }
        self.clear_interrupt_pending(res.1)?;
        let (hif, _len, address) = self.read_hif_header(res.1)?;
        match hif {
            HifGroup::Wifi(e) => match e {
                WifiResponse::CurrrentRssi => {
                    let mut result = [0xff; 4];
                    self.read_block(address, &mut result)?;
                    self.listener.on_rssi(result[0] as i8);
                    listener.on_rssi(result[0] as i8)
                }
                WifiResponse::DefaultConnect => {
                    let mut def_connect = [0xff; 4];
                    self.read_block(address, &mut def_connect)?;
                    self.listener.on_default_connect(def_connect[0] == 0);
                    listener.on_default_connect(def_connect[0] == 0)
                }
                WifiResponse::DhcpConf => {
                    let mut result = [0xff; 20];
                    self.read_block(address, &mut result)?;
                    self.listener.on_dhcp(read_dhcp_conf(&result)?);
                    listener.on_dhcp(read_dhcp_conf(&result)?)
                }
                WifiResponse::ConStateChanged => {
                    let mut connstate = [0xff; 4];
                    self.read_block(address, &mut connstate)?;
                    self.listener
                        .on_connstate_changed(connstate[0].into(), connstate[1].into());
                    listener.on_connstate_changed(connstate[0].into(), connstate[1].into());
                }
                WifiResponse::ConnInfo => {
                    let mut conninfo = [0xff; 48];
                    self.read_block(address, &mut conninfo)?;
                    self.listener.on_connection_info(conninfo.into());
                    listener.on_connection_info(conninfo.into())
                }
                WifiResponse::ScanResult => {
                    let mut result = [0xff; 44];
                    self.read_block(address, &mut result)?;
                    self.listener.on_scan_result(result.into());
                    listener.on_scan_result(result.into())
                }
                WifiResponse::ScanDone => {
                    let mut result = [0xff; 0x4];
                    self.read_block(address, &mut result)?;
                    self.listener.on_scan_done(result[0], result[1].into());
                    listener.on_scan_done(result[0], result[1].into())
                }
                WifiResponse::ClientInfo => {
                    unimplemented!("PS mode not yet supported")
                }
                // could translate to embedded-time, or core::Duration. No core::Systemtime exists
                // or chrono::
                WifiResponse::GetSysTime => {
                    let mut result = [0xff; 8];
                    self.read_block(address, &mut result)?;
                    self.listener.on_system_time(
                        (result[1] as u16 * 256u16) + result[0] as u16,
                        result[2],
                        result[3],
                        result[4],
                        result[5],
                        result[6],
                    );
                    listener.on_system_time(
                        (result[1] as u16 * 256u16) + result[0] as u16,
                        result[2],
                        result[3],
                        result[4],
                        result[5],
                        result[6],
                    );
                }
                WifiResponse::IpConflict => {
                    // replies with 4 bytes of conflicted IP
                    let mut result = [0xff; 4];
                    self.read_block(address, &mut result)?;
                    self.listener
                        .on_ip_conflict(u32::from_be_bytes(result).into());
                    listener.on_ip_conflict(u32::from_be_bytes(result).into());
                }
                WifiResponse::ProvisionInfo => {
                    unimplemented!("Provisioning not yet supported")
                }
                WifiResponse::GetPrng => {
                    unimplemented!("PRNG request not yet supported")
                }
                WifiResponse::Unhandled => {
                    panic!("Unhandled Wifi HIF")
                }
            },
            HifGroup::Ip(e) => match e {
                IpCode::DnsResolve => {
                    let mut result = [0; 68];
                    self.read_block(address, &mut result)?;
                    let rep = read_dns_reply(&result)?;
                    listener.on_resolve(rep.0, &rep.1);
                }
                IpCode::Ping => {
                    let mut result = [0; 20];
                    self.read_block(address, &mut result)?;
                    let rep = read_ping_reply(&result)?;
                    listener.on_ping(rep.0, rep.1, rep.2, rep.3, rep.4, rep.5)
                }
                IpCode::Bind => {
                    let mut result = [0; 4];
                    self.read_block(address, &mut result)?;
                    let rep = read_common_socket_reply(&result)?;
                    listener.on_bind(rep.0, rep.1);
                }
                IpCode::Listen => {
                    let mut result = [0; 4];
                    self.read_block(address, &mut result)?;
                    let rep = read_common_socket_reply(&result)?;
                    listener.on_listen(rep.0, rep.1);
                }
                IpCode::Accept => {
                    let mut result = [0; 12];
                    self.read_block(address, &mut result)?;
                    let rep = read_accept_reply(&result)?;
                    listener.on_accept(rep.0, rep.1, rep.2, rep.3);
                }
                IpCode::Connect => {
                    let mut result = [0; 4];
                    self.read_block(address, &mut result)?;
                    let rep = read_common_socket_reply(&result)?;
                    listener.on_connect(rep.0, rep.1)
                }
                IpCode::SendTo => {
                    let mut result = [0; 8];
                    self.read_block(address, &mut result)?;
                    let rep = read_send_reply(&result)?;
                    listener.on_send_to(rep.0, rep.1)
                }
                IpCode::Send => {
                    let mut result = [0; 8];
                    self.read_block(address, &mut result)?;
                    let rep = read_send_reply(&result)?;
                    listener.on_send(rep.0, rep.1)
                }
                IpCode::Recv => {
                    let mut buffer = [0; SOCKET_BUFFER_MAX_LENGTH];
                    let rep = self.get_recv_reply(address, &mut buffer)?;
                    listener.on_recv(rep.0, rep.1, rep.2, rep.3)
                }
                IpCode::RecvFrom => {
                    let mut buffer = [0; SOCKET_BUFFER_MAX_LENGTH];
                    let rep = self.get_recv_reply(address, &mut buffer)?;
                    listener.on_recvfrom(rep.0, rep.1, rep.2, rep.3)
                }
                IpCode::Close => {
                    unimplemented!("There is no response for close")
                }
                IpCode::SetSocketOption => {
                    unimplemented!("There is no response for setsockoption")
                }
                IpCode::Unhandled => {
                    panic!("Received unhandled HIF code {:?}", e)
                }
            },
            _ => panic!("Unexpected hif"),
        }
        Ok(())
    }

    // #endregion write
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hif_header() {
        let hif_header = [0x01u8, 0x2C, 0x16, 0x00];
        assert_eq!(
            hif_header_parse(hif_header),
            Ok((HifGroup::Wifi(WifiResponse::ConStateChanged), 22))
        );
    }

    pub struct Evt;
    impl EventListener for Evt {}

    type ByteWrite<'a> = &'a mut [u8];

    fn make_manager<'a>(writer: ByteWrite<'a>, cb: Evt) -> Manager<ByteWrite<'a>, Evt> {
        let mut mgr = Manager::from_xfer(writer, cb);
        mgr.chip.verify = false;
        mgr.chip.crc = false;
        mgr.chip.check_crc = false;
        mgr
    }
    const CMD_OFFSET: usize = 53;
    const DATA_OFFSET: usize = 73;

    #[test]
    fn test_close() {
        let mut buff = [0u8; 90];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});
        assert_eq!(mgr.send_close(Socket::new(67, 512 + 42)).unwrap(), ());
        assert_eq!(buff[CMD_OFFSET], 0x49);
        let theslice = &buff[DATA_OFFSET..DATA_OFFSET + 4];
        assert_eq!(theslice, &[67, 0, 42, 2]);
    }

    #[test]
    fn test_ping() {
        let mut buff = [0u8; 100];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});

        assert_eq!(
            mgr.send_ping_req(Ipv4Addr::new(192, 168, 5, 196), 42, 512 + 5, 0xDA),
            Ok(())
        );
        assert_eq!(buff[CMD_OFFSET], 0x52);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 12];
        assert_eq!(
            slice,
            &[
                192, 168, 5, 196, // ip
                0xDA, 0xBE, 0xBE, 0xBE, //marker
                5, 2,  // count
                42, // ttl
                0,
            ]
        );
    }

    #[test]
    fn test_bind() {
        let mut buff = [0u8; 100];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});
        assert_eq!(
            mgr.send_bind(
                Socket::new(42, 512 + 10),
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 196), 0xBADE)
            ),
            Ok(())
        );
        assert_eq!(buff[CMD_OFFSET], 0x41);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 12];
        assert_eq!(
            slice,
            &[
                2, 0, // addres family
                0xBA, 0xDE, // port
                192, 168, 5, 196, // ip
                42, 0, //socket + dummy
                10, 2 // session
            ]
        )
    }

    #[test]
    fn test_listen() {
        let mut buff = [0u8; 100];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});
        assert_eq!(mgr.send_listen(Socket::new(7, 512 + 10), 42), Ok(()));
        assert_eq!(buff[CMD_OFFSET], 0x42);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 4];
        assert_eq!(slice, &[7, 42, 10, 2])
    }

    #[test]
    fn test_connnect() {
        let mut buff = [0u8; 100];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});

        assert_eq!(
            mgr.send_socket_connect(
                Socket::new(7, 522),
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 196), 0xBADE)
            ),
            Ok(())
        );
        assert_eq!(buff[CMD_OFFSET], 0x44);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 12];
        assert_eq!(
            slice,
            &[
                2, 0, // address family
                0xBA, 0xDE, // port
                192, 168, 5, 196, // ip
                7, 0, // socket, ssl_flags
                10, 2 // session
            ]
        );
    }

    #[test]
    fn test_sendto() {
        let mut buff = [0u8; 120];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});

        assert_eq!(
            mgr.send_sendto(
                Socket::new(7, 522),
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 5, 196), 0xBADE),
                &[42]
            ),
            Ok(())
        );
        assert_eq!(buff[CMD_OFFSET], 0x47);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 16];
        assert_eq!(
            slice,
            &[
                7, 0, // socket, dummy
                1, 0, // length
                2, 0, // address family
                0xBA, 0xDE, // port
                192, 168, 5, 196, // ip
                10, 2, // session,
                0, 0 // dummy
            ]
        );
    }

    #[test]
    fn test_send() {
        let mut buff = [0u8; 120];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});

        assert_eq!(mgr.send_send(Socket::new(7, 522), &[42]), Ok(()));
        assert_eq!(buff[CMD_OFFSET], 0x45);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 16];
        assert_eq!(
            slice,
            &[
                7, 0, // socket, dummy
                1, 0, // length
                2, 0, // address family
                0, 0, 0, 0, 0, 0, // port + ip zeroed
                10, 2, // session,
                0, 0 // dummy
            ]
        );
    }

    #[test]
    fn test_recv() {
        let mut buff = [0u8; 120];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});

        assert_eq!(mgr.send_recv(Socket::new(7, 522), 0x01020304), Ok(()));
        assert_eq!(buff[CMD_OFFSET], 0x46);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 8];
        assert_eq!(
            slice,
            &[
                4, 3, 2, 1, // timeout
                7, 0, // socket+dummy
                10, 2 // session
            ]
        );
    }
    #[test]
    fn test_recvfrom() {
        let mut buff = [0u8; 120];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});

        assert_eq!(mgr.send_recvfrom(Socket::new(7, 522), 0x01020304), Ok(()));
        assert_eq!(buff[CMD_OFFSET], 0x48);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 8];
        assert_eq!(
            slice,
            &[
                4, 3, 2, 1, // timeout
                7, 0, // socket+dummy
                10, 2 // session
            ]
        );
    }
    #[test]
    fn test_recv_reply() {
        let mut buff = [1u8; 60];
        const OFFSET: usize = 10;
        buff[OFFSET + 0] = 2;
        buff[OFFSET + 1] = 0;
        buff[OFFSET + 8] = 0xF4; // set negative status
        buff[OFFSET + 9] = 0xFF;
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer, Evt {});

        let mut test = [2u8; 20];
        let (socket, _, dataslice, err) = mgr.get_recv_reply(2, &mut test).unwrap();
        assert_eq!(socket, Socket::new(1, 257));
        assert_eq!(err, SocketError::ConnAborted);
        assert_eq!(dataslice, &[]);
    }
}
