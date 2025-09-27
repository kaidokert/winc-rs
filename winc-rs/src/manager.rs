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

/// Low-level chip manager
use crate::errors::CommError as Error;
use core::fmt::Debug;

use crate::socket::Socket;
use crate::transfer::Xfer;

mod chip_access;
mod constants;
mod event_listener;
mod net_types;
mod requests;
mod responses;
use crate::{debug, error, trace};

use chip_access::ChipAccess;
#[cfg(feature = "experimental-ota")]
pub use constants::OtaUpdateError;
#[cfg(feature = "wep")]
pub use constants::WepKeyIndex;
pub use constants::{AuthType, PingError, SocketError, WifiChannel, WifiConnError, WifiConnState}; // todo response shouldn't be leaking
use constants::{IpCode, Regs, WifiRequest, WifiResponse};

#[cfg(feature = "flash-rw")]
pub(crate) use constants::FLASH_PAGE_SIZE;
#[cfg(feature = "experimental-ota")]
pub(crate) use constants::{OtaRequest, OtaResponse, OtaUpdateStatus};
pub(crate) use constants::{PRNG_DATA_LENGTH, SOCKET_BUFFER_MAX_LENGTH};

pub use net_types::{
    AccessPoint, Credentials, HostName, ProvisioningInfo, S8Password, S8Username, SocketOptions,
    Ssid, SslSockOpts, TcpSockOpts, UdpSockOpts, WpaKey,
};

#[cfg(feature = "wep")]
pub use net_types::WepKey;

use requests::*;
pub use responses::IPConf;
use responses::*;
pub use responses::{ConnectionInfo, ScanResult};

use core::net::{Ipv4Addr, SocketAddrV4};

pub use responses::FirmwareInfo;

/// HIF Response Group.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Default)]
pub(crate) enum HifGroup {
    #[default]
    Unhandled,
    Wifi(WifiResponse),
    Ip(IpCode),
    #[cfg(feature = "experimental-ota")]
    Ota(OtaResponse),
}

/// HIF Request Group.
#[derive(Copy, Clone)]
enum HifRequest {
    Wifi(WifiRequest),
    Ip(IpCode),
    #[cfg(feature = "experimental-ota")]
    Ota(OtaRequest),
}

/// Implementation to convert `HifRequest` to `u8` value.
impl From<HifRequest> for u8 {
    fn from(v: HifRequest) -> Self {
        match v {
            HifRequest::Wifi(_) => 1,
            HifRequest::Ip(_) => 2,
            #[cfg(feature = "experimental-ota")]
            HifRequest::Ota(_) => 4,
        }
    }
}

/// Implementation to convert `[u8; 2]` array to `HifGroup` value.
impl From<[u8; 2]> for HifGroup {
    fn from(v: [u8; 2]) -> Self {
        match v[0] {
            1 => Self::Wifi(v[1].into()),
            2 => Self::Ip(v[1].into()),
            #[cfg(feature = "experimental-ota")]
            4 => Self::Ota(v[1].into()),
            _ => Self::Unhandled,
        }
    }
}

/// Implementation to convert `HifGroup` to `u8` value.
impl From<HifGroup> for u8 {
    fn from(v: HifGroup) -> Self {
        match v {
            HifGroup::Wifi(_) => 1,
            HifGroup::Ip(_) => 2,
            #[cfg(feature = "experimental-ota")]
            HifGroup::Ota(_) => 4,
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
const HIF_SEND_RETRIES: usize = 1000;
#[cfg(feature = "flash-rw")]
const FLASH_REG_READ_RETRIES: usize = 10;
#[cfg(feature = "flash-rw")]
const FLASH_DUMMY_VALUE: u32 = 0x1084;

// todo this needs to be used
#[allow(dead_code)]
const TCP_SOCK_MAX: usize = 7;
#[allow(dead_code)]
const UDP_SOCK_MAX: usize = 4;
#[allow(dead_code)]
const MAX_SOCKET: usize = TCP_SOCK_MAX + UDP_SOCK_MAX;

pub trait EventListener {
    fn on_rssi(&mut self, level: i8);
    fn on_resolve(&mut self, ip: Ipv4Addr, host: &str);
    fn on_default_connect(&mut self, status: WifiConnError);
    fn on_dhcp(&mut self, conf: IPConf);
    fn on_connstate_changed(&mut self, state: WifiConnState, err: WifiConnError);
    fn on_connection_info(&mut self, info: ConnectionInfo);
    fn on_scan_result(&mut self, result: ScanResult);
    fn on_scan_done(&mut self, num_aps: u8, err: WifiConnError);
    fn on_system_time(&mut self, year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8);
    fn on_ip_conflict(&mut self, ip: Ipv4Addr);
    fn on_ping(
        &mut self,
        ip: Ipv4Addr,
        token: u32,
        rtt: u32,
        num_successful: u16,
        num_failed: u16,
        error: PingError,
    );
    fn on_bind(&mut self, sock: Socket, err: SocketError);
    fn on_listen(&mut self, sock: Socket, err: SocketError);
    fn on_accept(
        &mut self,
        address: SocketAddrV4,
        listen_socket: Socket,
        accepted_socket: Socket,
        data_offset: u16,
    );
    fn on_connect(&mut self, socket: Socket, err: SocketError);
    fn on_send_to(&mut self, socket: Socket, len: i16);
    fn on_send(&mut self, socket: Socket, len: i16);
    fn on_recv(&mut self, socket: Socket, address: SocketAddrV4, data: &[u8], err: SocketError);
    fn on_recvfrom(&mut self, socket: Socket, address: SocketAddrV4, data: &[u8], err: SocketError);
    fn on_prng(&mut self, data: &[u8]);
    fn on_provisioning(&mut self, ssid: Ssid, key: WpaKey, security: AuthType, status: bool);
    #[cfg(feature = "experimental-ota")]
    fn on_ota(&mut self, status: OtaUpdateStatus, error: OtaUpdateError);
}

pub struct Manager<X: Xfer> {
    // cached addresses
    not_a_reg_ctrl_4_dma: u32, // todo: make this dynamic/proper
    chip: ChipAccess<X>,
}

/// The stages of the boot process
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum BootStage {
    Start,
    StartBootrom,
    Stage2,
    Stage3,
    Stage4,
    StageStartFirmware,
    FinishFirmwareBoot,
}

/// Stores boot state for the long-running boot
/// process
pub(crate) struct BootState {
    stage: BootStage,
    loop_value: u32,
}
impl Default for BootState {
    fn default() -> Self {
        Self {
            stage: BootStage::Start,
            loop_value: 0,
        }
    }
}
impl<X: Xfer> Manager<X> {
    pub fn from_xfer(xfer: X) -> Self {
        Self {
            not_a_reg_ctrl_4_dma: 0xbf0000,
            chip: ChipAccess::new(xfer),
        }
    }
    #[cfg(test)]
    pub fn set_unit_test_mode(&mut self) {
        self.chip.set_unit_test_mode();
    }
    // Todo: remove this
    pub fn delay_us(&mut self, delay: u32) {
        self.chip.delay_us(delay);
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

    #[allow(dead_code)] // todo
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

    /// Resets the chip.
    ///
    /// # Returns
    ///
    /// * `()` - If the chip was successfully reset.
    /// * `Error` - If an error occurs while resetting the chip.
    pub(crate) fn chip_reset(&mut self) -> Result<(), Error> {
        self.chip.single_reg_write(Regs::ChipReset.into(), 0)?;
        // back-off delay
        self.chip.delay_us(50_000); // 50 msec delay

        Ok(())
    }

    /// Halt the chip.
    ///
    /// # Returns
    ///
    /// * `()` - If the chip was successfully halted.
    /// * `Error` - If an error occurs while halting the chip.
    pub(crate) fn chip_halt(&mut self) -> Result<(), Error> {
        const HALT_BIT: u32 = 1 << 0; // 0x01
        const RESET_BIT: u32 = 1 << 10; // 0x400

        let mut reg = self.chip.single_reg_read(Regs::ChipHalt.into())?;

        self.chip
            .single_reg_write(Regs::ChipHalt.into(), reg | HALT_BIT)?;

        reg = self.chip.single_reg_read(Regs::ChipReset.into())?;

        if (reg & RESET_BIT) == RESET_BIT {
            reg &= !RESET_BIT;

            self.chip.single_reg_write(Regs::ChipReset.into(), reg)?;
            _ = self.chip.single_reg_read(Regs::ChipReset.into())?;
        }

        Ok(())
    }

    /// Resets the SPI bus
    ///
    /// # Returns
    ///
    /// * `()` - If the bus was reset successfully.
    /// * `Error` - If an error occurs while resetting the SPI bus.
    pub(crate) fn spi_bus_reset(&mut self) -> Result<(), Error> {
        self.chip.bus_reset()
    }

    /// Wake up the chip.
    ///
    /// # Returns
    ///
    /// * `()` - If the chip is successfully woken up.
    /// * `Error` - If any error occurs while waking up the chip.
    pub(crate) fn chip_wake(&mut self) -> Result<(), Error> {
        const WAKEUP_BIT: u32 = 1 << 0; // 0x01
        const WAKEUP_CLK_BIT: u32 = 1 << 1; // 0x02
        const CLK_EN_BIT: u32 = 1 << 2; // 0x04
        const WAKEUP_DELAY_USEC: u32 = 2000; // 2 msec delay

        let mut reg = self.chip.single_reg_read(Regs::HostToCortusComm.into())?;

        // bit 0 indicates host wakeup
        if (reg & WAKEUP_BIT) == 0 {
            self.chip
                .single_reg_write(Regs::HostToCortusComm.into(), reg | WAKEUP_BIT)?;
        }

        reg = self.chip.single_reg_read(Regs::WakeClock.into())?;
        // Set the WAKEUP_CLK_BIT (bit 1); hardware will assert CLK_EN_BIT when ready.
        if (reg & WAKEUP_CLK_BIT) == 0 {
            self.chip
                .single_reg_write(Regs::WakeClock.into(), reg | WAKEUP_CLK_BIT)?;
        }

        let mut retries = 4u8;
        loop {
            if retries == 0 {
                error!("Reading enable clock register timed out.");
                return Err(Error::OperationRetriesExceeded);
            }

            reg = self.chip.single_reg_read(Regs::EnableClock.into())?;

            if (reg & CLK_EN_BIT) > 0 {
                break;
            }

            retries -= 1;
            // backoff delay
            self.chip.delay_us(WAKEUP_DELAY_USEC);
        }

        // reset spi bus
        self.spi_bus_reset()
    }

    /// Boots the chip into normal mode.
    ///
    /// # Arguments
    ///
    /// * `state` - Updated boot state.
    ///
    /// # Returns
    ///
    /// * `bool` - Whether the chip completed boot (true) or is still booting (false).
    /// * `Error` - If an error occurs during the boot process.
    pub(crate) fn boot_the_chip(&mut self, state: &mut BootState) -> Result<bool, Error> {
        const MAX_LOOPS: u32 = 10;
        const FINISH_BOOT_ROM: u32 = 0x10add09e;
        debug!("Waiting for chip start .. stage: {:?}", state.stage);
        match state.stage {
            BootStage::Start => {
                debug!("chip id {:x} rev:{:x}", self.chip_id()?, self.chip_rev()?);
                self.configure_spi_packetsize()?;
                state.stage = BootStage::StartBootrom;
                state.loop_value = 0;
            }
            BootStage::StartBootrom => {
                if state.loop_value >= MAX_LOOPS {
                    return Err(Error::BootRomStart);
                }
                let efuse = self.chip.single_reg_read(Regs::EFuseRead.into())? & 0x80000000;
                if efuse != 0 {
                    state.stage = BootStage::Stage2;
                }
            }
            BootStage::Stage2 => {
                let host_wait = self.chip.single_reg_read(Regs::WaitForHost.into())? & 0x1;
                if host_wait != 0 {
                    state.stage = BootStage::Stage4;
                } else {
                    state.stage = BootStage::Stage3;
                    state.loop_value = 0;
                }
            }
            BootStage::Stage3 => {
                if state.loop_value >= MAX_LOOPS {
                    return Err(Error::BootRomStart);
                }
                let host_wait = self.chip.single_reg_read(Regs::BootRom.into())?;
                if host_wait == FINISH_BOOT_ROM {
                    state.stage = BootStage::Stage4;
                }
            }
            BootStage::Stage4 => {
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
                state.stage = BootStage::StageStartFirmware;
                state.loop_value = 0;
            }
            BootStage::StageStartFirmware => {
                if state.loop_value >= MAX_LOOPS {
                    return Err(Error::FirmwareStart);
                }
                const FINISH_INIT: u32 = 0x02532636;
                self.delay_us(2 * 1000); // 2 msec
                let reg = self.chip.single_reg_read(Regs::NmiState.into())?;
                if reg == FINISH_INIT {
                    state.stage = BootStage::FinishFirmwareBoot;
                }
            }
            BootStage::FinishFirmwareBoot => {
                self.chip.single_reg_write(Regs::NmiState.into(), 0)?;
                self.enable_interrupt_pins()?;
                // After chip boot, we can go a lot faster
                self.chip.switch_to_high_speed();
                return Ok(true);
            }
        }
        state.loop_value += 1;
        Ok(false)
    }

    pub fn configure_spi_packetsize(&mut self) -> Result<(), Error> {
        let mut conf = self.chip.single_reg_read(Regs::SpiConfig.into())?;
        conf &= 0xFFFFFF0F; // clear
        conf |= 0x00000050; // set to 8k packet size
        self.chip.single_reg_write(Regs::SpiConfig.into(), conf)?;
        trace!("Set spiconfig to {:x}", conf);
        Ok(())
    }

    /// Enables interrupts on the module's pins.
    ///
    /// # Returns
    ///
    /// * `()` - If the interrupts were successfully enabled.
    /// * `Error` - If an error occurs while enabling the interrupts.
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

    /// Disables all the interrupts in the module.
    ///
    /// # Returns
    ///
    /// * `()` - If the interrupts were successfully disabled.
    /// * `Error` - If an error occurs while disabling the interrupts.
    pub(crate) fn disable_internal_interrupt(&mut self) -> Result<(), Error> {
        self.chip.single_reg_write(Regs::CortusIrq.into(), 0)
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
    /// Sends the HIF header.
    ///
    /// # Arguments
    ///
    /// * `gid` - HIF Group ID. (e.g., WiFi, IP, OTA, HIF).
    /// * `op` - Operation ID.
    /// * `len` - Length of the data/control packet to send.
    /// * `req_data` - Indicates whether to request data from the chip.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the HIF header is successfully sent to the chip.
    /// * `Err(Error)` - If any error occurred while preparing or sending the HIF header.
    fn prep_for_hif_send(
        &mut self,
        gid: u8,
        op: u8,
        len: u16,
        req_data: bool,
    ) -> Result<(), Error> {
        // Write NMI state
        let mut state: u32 = 0;
        state |= gid as u32;
        state |= if req_data {
            ((op | 0x80) as u32) << 8
        } else {
            (op as u32) << 8
        };
        state |= (len as u32) << 16;
        self.chip.single_reg_write(Regs::NmiState.into(), state)?;
        // Set RCV_CTRL_2 bit 1
        self.chip
            .single_reg_write(Regs::WifiHostRcvCtrl2.into(), 2)?;

        // Wait for bit 1 in RCV_CTRL_2 to clear, with timeout
        let mut retries = HIF_SEND_RETRIES;
        let mut res;
        loop {
            res = self.chip.single_reg_read(Regs::WifiHostRcvCtrl2.into())?;
            res &= 2;
            if res == 0 || retries == 0 {
                break;
            }
            // TODO: There should be a small back-off delay here
            // perhaps add "delay" to Xfer trait and call into it
            retries -= 1;
        }
        if res != 0 {
            return Err(Error::HifSendFailed);
        }
        // Read DMA address from RCV_CTRL_4
        self.not_a_reg_ctrl_4_dma = self.chip.single_reg_read(Regs::WifiHostRcvCtrl4.into())?;
        trace!("Dma address: {:x}", self.not_a_reg_ctrl_4_dma);
        Ok(())
    }

    /// Prepares and writes the HIF header.
    ///
    /// # Arguments
    ///
    /// * `req` - HIF request (e.g., WiFi, IP, OTA, HIF).
    /// * `payload` - The request/control buffer to be sent. Maximum length: 65,535 bytes.
    /// * `req_data` - Indicates whether request data is expected from the chip.
    /// * `data_packet` - Optional. A tuple containing the size of the data to send and its offset. Maximum value: 65,535.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the HIF header was successfully prepared and sent to the chip.
    /// * `Err(Error)` - If an error occurred while preparing or writing the HIF header.
    fn write_hif_header_impl(
        &mut self,
        req: HifRequest,
        payload: &[u8],
        req_data: bool,
        data_packet: Option<(usize /* Data Size */, usize /* Offset */)>,
    ) -> Result<(), Error> {
        // Length of packet to send.
        let len = match data_packet {
            Some((size, offset)) => HIF_HEADER_OFFSET + size + offset,
            None => payload.len() + HIF_HEADER_OFFSET,
        };

        if len > u16::MAX as usize {
            error!(
                "The length of the data/control packet exceeds the maximum value: expected up to 65,535, got {}",
                len
            );
            return Err(Error::BufferError);
        }

        let pkglen = (len as u16).to_le_bytes();

        // Operation ID.
        let opp: u8 = match req {
            HifRequest::Wifi(opcode) => opcode.into(),
            HifRequest::Ip(opcode) => opcode.into(),
            #[cfg(feature = "experimental-ota")]
            HifRequest::Ota(opcode) => opcode.into(),
        };
        // Group ID.
        let grpval = req.into();
        self.prep_for_hif_send(grpval, opp, len as u16, req_data)?;

        self.chip.dma_block_write(
            self.not_a_reg_ctrl_4_dma,
            &[
                grpval, opp, pkglen[0], pkglen[1], 0x00, // unused bytes
                0x00, 0x00, 0x00,
            ],
        )
    }

    /// Prepares and writes the HIF header without a data packet.
    ///
    /// # Arguments
    ///
    /// * `req` - The HIF request type (e.g., WiFi, IP, OTA, HIF).
    /// * `payload` - The request/control buffer to be sent. Maximum length: 65,535 bytes.
    /// * `req_data` - Indicates whether request data is expected from the chip.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the HIF header was successfully prepared and sent to the chip.
    /// * `Err(Error)` - If an error occurred while preparing or writing the HIF header.
    fn write_hif_header(
        &mut self,
        req: HifRequest,
        payload: &[u8],
        req_data: bool,
    ) -> Result<(), Error> {
        self.write_hif_header_impl(req, payload, req_data, None)
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
        self.write_hif_header(HifRequest::Wifi(WifiRequest::DefaultConnect), &[], false)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a Connect request to chip.
    ///
    /// # Arguments
    ///
    /// * `ssid` - The SSID (network name), up to 32 bytes.
    /// * `credentials` - Security credentials (e.g., passphrase or authentication data).
    /// * `channel` - Wi-Fi RF channel.
    /// * `dont_save_credentials` - Whether to save credentials or not.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurred while preparing or sending the connect request.
    pub fn send_connect(
        &mut self,
        ssid: &Ssid,
        credentials: &Credentials,
        channel: WifiChannel,
        dont_save_credentials: bool,
    ) -> Result<(), Error> {
        let arr = write_connect_request(ssid, credentials, channel, dont_save_credentials)?;
        self.write_hif_header(HifRequest::Wifi(WifiRequest::Connect), &arr, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &arr)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_get_current_rssi(&mut self) -> Result<(), Error> {
        self.write_hif_header(HifRequest::Wifi(WifiRequest::CurrentRssi), &[], false)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_get_conn_info(&mut self) -> Result<(), Error> {
        self.write_hif_header(HifRequest::Wifi(WifiRequest::GetConnInfo), &[], false)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }
    pub fn send_scan(&mut self, channel: u8, scantime: u16) -> Result<(), Error> {
        let req = write_scan_req(channel, scantime)?;
        self.write_hif_header(HifRequest::Wifi(WifiRequest::Scan), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_get_scan_result(&mut self, index: u8) -> Result<(), Error> {
        let req = [index, 0, 0, 0];
        self.write_hif_header(HifRequest::Wifi(WifiRequest::ScanResult), &req, false)?;
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
        self.write_hif_header(HifRequest::Ip(IpCode::Ping), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_gethostbyname(&mut self, host: &str) -> Result<(), Error> {
        let mut buffer = [0x0u8; 64];
        let req = write_gethostbyname_req(host, &mut buffer)?;
        self.write_hif_header(HifRequest::Ip(IpCode::DnsResolve), req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }
    pub fn send_bind(&mut self, socket: Socket, address: SocketAddrV4) -> Result<(), Error> {
        // todo: address family is useless here
        let req = write_bind_req(socket, 2, address)?;
        self.write_hif_header(HifRequest::Ip(IpCode::Bind), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }
    pub fn send_listen(&mut self, socket: Socket, backlog: u8) -> Result<(), Error> {
        let req = write_listen_req(socket, backlog)?;
        self.write_hif_header(HifRequest::Ip(IpCode::Listen), &req, false)?;
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
        self.write_hif_header(HifRequest::Ip(IpCode::Connect), &req, false)?;
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
        self.write_hif_header(HifRequest::Ip(IpCode::SendTo), &req, true)?;
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
        self.write_hif_header(HifRequest::Ip(IpCode::Send), &req, true)?;
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
        self.write_hif_header(HifRequest::Ip(IpCode::Recv), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_recvfrom(&mut self, socket: Socket, timeout: u32) -> Result<(), Error> {
        let req = write_recv_req(socket, timeout)?;
        self.write_hif_header(HifRequest::Ip(IpCode::RecvFrom), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    pub fn send_close(&mut self, socket: Socket) -> Result<(), Error> {
        let req = write_close_req(socket)?;
        self.write_hif_header(HifRequest::Ip(IpCode::Close), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Send a set socket option request to module.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket to which the option will be applied.
    /// * `option` - A reference to the UDP socket option to be set.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurred while preparing or sending the request.
    pub fn send_setsockopt(&mut self, socket: Socket, option: &UdpSockOpts) -> Result<(), Error> {
        let req = write_setsockopt_req(socket, (*option).into(), option.get_value())?;
        self.write_hif_header(HifRequest::Ip(IpCode::SetSocketOption), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Send a set SSL socket option request to module.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket to which the option will be applied.
    /// * `option` - A reference to the SSL socket option to be set.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurred while preparing or sending the request.
    pub fn send_ssl_setsockopt(
        &mut self,
        socket: Socket,
        option: &SslSockOpts,
    ) -> Result<(), Error> {
        let req = write_ssl_setsockopt_req(socket, option)?;

        self.write_hif_header(HifRequest::Ip(IpCode::SslSetSockOpt), &req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Send a disconnect request to module.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurred while preparing or sending the request.
    pub fn send_disconnect(&mut self) -> Result<(), Error> {
        self.write_hif_header(HifRequest::Wifi(WifiRequest::Disconnect), &[], false)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a PRNG request to the chip.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the input buffer where the PRNG data will be stored.
    /// * `len` - The length of the input buffer, i.e., the number of random bytes to generate.
    ///
    /// # Warning
    ///
    /// * It is recommended to pass the address of a valid memory location rather than
    ///   an arbitrary one, to avoid potential memory leaks or data corruption.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurred during the PRNG packet request or preparation.
    pub fn send_prng(&mut self, addr: u32, len: u16) -> Result<(), Error> {
        let req = write_prng_req(addr, len)?;
        self.write_hif_header(HifRequest::Wifi(WifiRequest::GetPrng), &req, true)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a request to start provisioning mode.
    ///
    /// # Arguments
    ///
    /// * `ap` - Configuration parameters for the access point, including SSID, password, authentication type, etc.
    /// * `dns` - DNS redirect URL as a string slice. Must not end with `.local`.
    /// * `http_redirect` - Enables or disables HTTP redirection. If enabled, all HTTP traffic
    ///   (`http://<URL>`) from devices connected to the WINC access point will be redirected
    ///   to the HTTP provisioning web page.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurs during packet preparation or sending.
    pub fn send_start_provisioning(
        &mut self,
        ap: &AccessPoint,
        hostname: &HostName,
        http_redirect: bool,
    ) -> Result<(), Error> {
        let req = write_start_provisioning_req(ap, hostname, http_redirect)?;
        self.write_hif_header(
            HifRequest::Wifi(WifiRequest::StartProvisionMode),
            &req,
            true,
        )?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a request to stop provisioning mode.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurs during packet preparation or transmission.
    pub fn send_stop_provisioning(&mut self) -> Result<(), Error> {
        self.write_hif_header(HifRequest::Wifi(WifiRequest::StopProvisionMode), &[], false)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a request to enable Access Point mode.
    ///
    /// # Arguments
    ///
    /// * `ap` - Configuration parameters for the access point, including SSID, password, authentication type, etc.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurs during packet preparation or sending.
    pub fn send_enable_access_point(&mut self, ap: &AccessPoint) -> Result<(), Error> {
        let req = write_en_ap_req(ap)?;
        self.write_hif_header(HifRequest::Wifi(WifiRequest::EnableAp), &req, true)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a request to disable Access Point mode.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurs during packet preparation or sending.
    pub fn send_disable_access_point(&mut self) -> Result<(), Error> {
        self.write_hif_header(HifRequest::Wifi(WifiRequest::DisableAp), &[], false)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    #[cfg(feature = "experimental-ota")]
    /// Send a request to start the OTA update for either winc1500 network stack or cortus processor.
    ///
    /// # Arguments
    ///
    /// * `server_url` - Server URL from where firware image will be downloaded.
    /// * `cortus_update` - Whether the OTA update is for cortus processor or winc1500 stack.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurs during packet preparation or sending.
    pub fn send_start_ota_update(
        &mut self,
        server_url: &[u8],
        cortus_update: bool,
    ) -> Result<(), Error> {
        // Check whether request is for Cortus or for network stack.
        let req_id = if cortus_update {
            OtaRequest::StartCortusFirmwareUpdate
        } else {
            OtaRequest::StartFirmwareUpdate
        };
        self.write_hif_header(HifRequest::Ota(req_id), server_url, false)?;
        self.chip.dma_block_write(
            self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32,
            server_url,
        )?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    #[cfg(feature = "experimental-ota")]
    /// Sends an OTA request (rollback, abort, switch) either for the
    /// WINC1500 network stack or the Cortus processor.
    ///
    /// # Arguments
    ///
    /// * `request` - Type of OTA request to send.
    ///
    /// # Returns
    ///
    /// * `()` - If the request is successfully sent.
    /// * `Error` - If an error occurs during packet preparation or sending.
    pub fn send_ota_request(&mut self, request: OtaRequest) -> Result<(), Error> {
        self.write_hif_header(HifRequest::Ota(request), &[], false)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    #[cfg(feature = "flash-rw")]
    /// Checks the flash data transfer register.
    ///
    /// # Returns
    ///
    /// * `()` - If the flash transfer is complete.
    /// * `Error` - If an error occurs while reading the register or the process times out.
    fn check_flash_tx_complete(&mut self) -> Result<(), Error> {
        let mut retries = FLASH_REG_READ_RETRIES;
        let mut res = self.chip.single_reg_read(Regs::FlashTransferDone.into())?;

        while res != 1 {
            if retries == 0 {
                error!("Reading flash transfer complete register timed out.");
                return Err(Error::OperationRetriesExceeded);
            }

            retries -= 1;

            res = self.chip.single_reg_read(Regs::FlashTransferDone.into())?;
        }

        Ok(())
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to write data (less than a page size) from Cortus memory to flash.
    ///
    /// # Arguments
    ///
    /// * `flash_addr` – The flash address where data will be written.
    /// * `data_size` – The size of the data to write.
    ///
    /// # Returns
    ///
    /// * `()` - The data was successfully written to flash.
    /// * `Error` - If an error occurs while writing the data from Cortus memory to flash.
    fn send_flash_write_page(&mut self, flash_addr: u32, data_size: usize) -> Result<(), Error> {
        if data_size > FLASH_PAGE_SIZE {
            return Err(Error::ExceedsFlashPageSize);
        }

        let cmd = {
            let b = flash_addr.to_be_bytes();
            [0x02, b[1], b[2], b[3]]
        };

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), u32::from_le_bytes(cmd))?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), 0x0F)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), Regs::FlashSharedMemory.into())?;

        // Mask data_size to 20 bits, shift to high bytes, and set 0x84 as the low byte
        let size = 0x84 | ((data_size & 0xfffff) << 8);

        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), size as u32)?;

        // read transfer complete register.
        self.check_flash_tx_complete()
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to read the flash status register.
    ///
    /// # Returns
    ///
    /// * `u8` – The value of the status register.
    /// * `Error` – If an error occurs while reading the status register.
    pub(crate) fn send_flash_read_status_register(&mut self) -> Result<u8, Error> {
        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x04)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), 0x05)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), 0x01)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), FLASH_DUMMY_VALUE)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), 0x81)?;

        // read transfer complete register.
        self.check_flash_tx_complete()?;

        let res = self.chip.single_reg_read(FLASH_DUMMY_VALUE)?;
        Ok((res & 0xff) as u8)
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to load data from flash into Cortus processor memory.
    ///
    /// # Arguments
    ///
    /// * `flash_addr` – The flash address to load data from.
    /// * `data_size` – The size of the data to load.
    ///
    /// # Returns
    ///
    /// * `()` - Data is successfully loaded into Cortus processor memory.
    /// * `Error` - If an error occurs while loading the flash data into Cortus memory.
    fn send_flash_load_data_to_cortus_memory(
        &mut self,
        flash_addr: u32,
        data_size: usize,
    ) -> Result<(), Error> {
        let cmd = {
            let b = flash_addr.to_be_bytes();
            [0x0b, b[1], b[2], b[3]]
        };

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), data_size as u32)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), u32::from_le_bytes(cmd))?;
        self.chip
            .single_reg_write(Regs::FlashBuffer2.into(), 0xA5)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), 0x1F)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), Regs::FlashSharedMemory.into())?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), 0x85)?;
        // read transfer complete register.
        self.check_flash_tx_complete()
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to erase a flash sector (4KB).
    ///
    /// # Arguments
    ///
    /// * `flash_addr` - The flash address of the sector to erase.
    ///
    /// # Returns
    ///
    /// * `()` - The flash sector was successfully erased.
    /// * `Error` - If an error occurs while erasing the flash sector.
    pub(crate) fn send_flash_erase_sector(&mut self, flash_addr: u32) -> Result<(), Error> {
        let cmd = {
            let b = flash_addr.to_be_bytes();
            [0x20, b[1], b[2], b[3]]
        };

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), u32::from_le_bytes(cmd))?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), 0x0F)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), 0)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), 0x84)?;

        // read transfer complete register.
        self.check_flash_tx_complete()
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to enable or disable write access to the flash.
    ///
    /// # Arguments
    ///
    /// * `enable` – `true` to enable write access; `false` to disable it.
    ///
    /// # Returns
    ///
    /// * `()` – Write access to the flash was successfully enabled or disabled.
    /// * `Error` – If an error occurs while sending the command to change write access.
    pub(crate) fn send_flash_write_access(&mut self, enable: bool) -> Result<(), Error> {
        let val = if enable { 0x06 } else { 0x04 };
        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip.single_reg_write(Regs::FlashBuffer1.into(), val)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), 0x01)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), 0x00)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), 0x81)?;
        // read transfer complete register.
        self.check_flash_tx_complete()
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to write data to a flash memory.
    ///
    /// # Arguments
    ///
    /// * `flash_addr` – The address in flash memory where the data will be written.
    /// * `data` – The data to write. Must not exceed the flash page size (256 bytes).
    ///
    /// # Returns
    ///
    /// * `()` - The data was successfully written to flash memory.
    /// * `Error` - If an error occurs while writing the data to flash.
    pub(crate) fn send_flash_write(&mut self, flash_addr: u32, data: &[u8]) -> Result<(), Error> {
        if data.is_empty() {
            error!("Invalid data buffer");
            return Err(Error::BufferError);
        }
        if data.len() > FLASH_PAGE_SIZE {
            error!("Data should not be greater than the page size, which is 256 bytes.");
            return Err(Error::ExceedsFlashPageSize);
        }
        // enable flash writing
        self.send_flash_write_access(true)?;
        // use shared memory
        self.chip
            .dma_block_write(Regs::FlashSharedMemory.into(), data)?;
        // set flash address
        self.send_flash_write_page(flash_addr, data.len())?;
        // read status register
        let mut retries = FLASH_REG_READ_RETRIES;
        let mut res = self.send_flash_read_status_register()?;

        while (res & 0x01) != 0 {
            if retries == 0 {
                return Err(Error::OperationRetriesExceeded);
            }

            retries -= 1;

            res = self.send_flash_read_status_register()?;
        }

        // disable writing to flash
        self.send_flash_write_access(false)
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to read the flash ID.
    ///
    /// # Returns
    ///
    /// * `u32` - The flash ID.
    /// * `Error` - If an error occurs while reading the flash ID.
    pub(crate) fn send_flash_read_id(&mut self) -> Result<u32, Error> {
        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x04)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), 0x9F)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), 0x01)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), FLASH_DUMMY_VALUE)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), 0x81)?;
        // read transfer complete register.
        self.check_flash_tx_complete()?;

        let value = self.chip.single_reg_read(FLASH_DUMMY_VALUE)?;

        Ok(value)
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to the flash to enter or exit low power mode.
    ///
    /// # Arguments
    ///
    /// * `enable` – `true` to enter low power mode; `false` to exit it.
    ///
    /// # Returns
    ///
    /// * `()` – The flash successfully entered or exited low power mode.
    /// * `Error` – If an error occurs while attempting to change the flash power mode.
    pub(crate) fn send_flash_low_power_mode(&mut self, enable: bool) -> Result<(), Error> {
        let val: u32 = if enable { 0xB9 } else { 0xAB };
        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip.single_reg_write(Regs::FlashBuffer1.into(), val)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), 0x01)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), 0)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), 0x81)?;
        // read transfer complete register.
        self.check_flash_tx_complete()
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to enable or disable flash pinmux.
    ///
    /// # Arguments
    ///
    /// * `enable` – `true` to enable pinmux; `false` to disable it.
    ///
    /// # Returns
    ///
    /// * `()` – Pinmux was successfully enabled or disabled on the flash.
    /// * `Error` – If an error occurs while enabling or disabling the flash pinmux.
    pub(crate) fn send_flash_pin_mux(&mut self, enable: bool) -> Result<(), Error> {
        const GPIO_PINS_MASK: u32 = 0x7777; // GPIO15/16/17/18
        const FLASH_PINMUX_ENABLE: u32 = 0x1111;
        const FLASH_PINMUX_DISABLE: u32 = 0x0010;

        let mut val = self.chip.single_reg_read(Regs::FlashPinMux.into())?;

        val &= !((GPIO_PINS_MASK) << 12);

        val |= if enable {
            (FLASH_PINMUX_ENABLE) << 12
        } else {
            (FLASH_PINMUX_DISABLE) << 12
        };

        self.chip.single_reg_write(Regs::FlashPinMux.into(), val)
    }

    #[cfg(feature = "flash-rw")]
    /// Sends a command to read data from flash memory.
    ///
    /// # Arguments
    ///
    /// * `flash_addr` – The address in flash memory to read from.
    /// * `buffer` – A mutable buffer where the read data will be stored.
    ///
    /// # Returns
    ///
    /// * `()` – Data was successfully read from flash memory.
    /// * `Error` – If an error occurs while reading data from the flash.
    pub(crate) fn send_flash_read(
        &mut self,
        flash_addr: u32,
        buffer: &mut [u8],
    ) -> Result<(), Error> {
        if buffer.is_empty() {
            return Err(Error::BufferError);
        }
        // load data to shared memory between flash and cortus processor.
        self.send_flash_load_data_to_cortus_memory(flash_addr, buffer.len())?;
        // read the data from th shared from memory
        self.chip
            .dma_block_read(Regs::FlashSharedMemory.into(), buffer)
    }
    // #endregion write
}

#[cfg(test)]
mod tests {
    use super::*;
    use constants::{ENABLE_AP_PACKET_SIZE, PRNG_PACKET_SIZE};

    #[test]
    fn test_hif_header() {
        let hif_header = [0x01u8, 0x2C, 0x16, 0x00];
        assert_eq!(
            hif_header_parse(hif_header),
            Ok((HifGroup::Wifi(WifiResponse::ConStateChanged), 22))
        );
    }

    type ByteWrite<'a> = &'a mut [u8];

    fn make_manager<'a>(writer: ByteWrite<'a>) -> Manager<ByteWrite<'a>> {
        let mut mgr = Manager::from_xfer(writer);
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
        let mut mgr = make_manager(&mut writer);
        assert_eq!(mgr.send_close(Socket::new(67, 512 + 42)).unwrap(), ());
        assert_eq!(buff[CMD_OFFSET], 0x49);
        let theslice = &buff[DATA_OFFSET..DATA_OFFSET + 4];
        assert_eq!(theslice, &[67, 0, 42, 2]);
    }

    #[test]
    fn test_ping() {
        let mut buff = [0u8; 100];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);

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
        let mut mgr = make_manager(&mut writer);
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
        let mut mgr = make_manager(&mut writer);
        assert_eq!(mgr.send_listen(Socket::new(7, 512 + 10), 42), Ok(()));
        assert_eq!(buff[CMD_OFFSET], 0x42);
        let slice = &buff[DATA_OFFSET..DATA_OFFSET + 4];
        assert_eq!(slice, &[7, 42, 10, 2])
    }

    #[test]
    fn test_connnect() {
        let mut buff = [0u8; 100];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);

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
        let mut mgr = make_manager(&mut writer);

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
        let mut mgr = make_manager(&mut writer);

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
        let mut mgr = make_manager(&mut writer);

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
        let mut mgr = make_manager(&mut writer);

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
        let mut mgr = make_manager(&mut writer);

        let mut test = [2u8; 20];
        let (socket, _, dataslice, err) = mgr.get_recv_reply(2, &mut test).unwrap();
        assert_eq!(socket, Socket::new(1, 257));
        assert_eq!(err, SocketError::ConnAborted);
        assert_eq!(dataslice, &[]);
    }

    #[test]
    fn test_prng() {
        let mut buff = [0u8; 100];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);

        assert_eq!(mgr.send_prng(0x2000_65DC, 16), Ok(()));

        assert_eq!(buff[CMD_OFFSET], WifiRequest::GetPrng.into());

        let slice = &buff[DATA_OFFSET..DATA_OFFSET + PRNG_PACKET_SIZE];

        assert_eq!(
            slice,
            &[
                0xDC, 0x65, 0x00, 0x020, // Address
                0x10, 0x00, // length
                0x00, 0x00 // void
            ]
        )
    }

    #[test]
    fn test_send_enable_ap() {
        let mut buff = [0u8; 300];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);
        let ssid = Ssid::from("ssid").unwrap();
        let key = WpaKey::from("password").unwrap();
        let ap = AccessPoint::wpa(&ssid, &key);

        assert_eq!(mgr.send_enable_access_point(&ap), Ok(()));

        assert_eq!(buff[CMD_OFFSET], WifiRequest::EnableAp.into());

        let slice = &buff[DATA_OFFSET..DATA_OFFSET + ENABLE_AP_PACKET_SIZE];

        assert_eq!(
            slice,
            &[
                // Ssid
                0x73, 0x73, 0x69, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // Wifi Channel
                0x00, // Wep Key Index
                0x08, // Wep/WPA Key Size
                // Wep Key
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x02, // Security Type
                0x00, // SSID Hidden
                0xC0, 0xA8, 0x01, 0x01, // AP IP
                // WPA Key
                0x70, 0x61, 0x73, 0x73, 0x77, 0x6F, 0x72, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Padding
                0x00, 0x00
            ]
        )
    }

    #[test]
    fn test_send_disable_ap() {
        let mut buff = [0u8; 73];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);

        assert_eq!(mgr.send_disable_access_point(), Ok(()));

        assert_eq!(buff[CMD_OFFSET], WifiRequest::DisableAp.into());
    }
}
