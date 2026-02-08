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
pub(crate) mod constants;
mod event_listener;
mod net_types;
mod registers;
mod requests;
mod responses;
use crate::{debug, error, trace};

use chip_access::ChipAccess;
#[cfg(feature = "experimental-ota")]
pub use constants::OtaUpdateError;

#[cfg(feature = "wep")]
pub use constants::WepKeyIndex;

pub use constants::{AuthType, PingError, SocketError, WifiChannel, WifiConnError, WifiConnState};
use constants::{IpCode, WifiRequest, WifiResponse, AF_INET};

#[cfg(feature = "flash-rw")]
pub(crate) use constants::FLASH_PAGE_SIZE;

#[cfg(feature = "flash-rw")]
pub(crate) use registers::{FLASH_READ_STATUS_BIT, FLASH_SIZE_INFO_SHIFT, LOW_12_BIT_MASK};

#[cfg(feature = "experimental-ota")]
pub(crate) use constants::{OtaRequest, OtaResponse, OtaUpdateStatus};

pub(crate) use constants::{BootMode, PRNG_DATA_LENGTH, SOCKET_BUFFER_MAX_LENGTH};

#[cfg(feature = "ssl")]
pub(crate) use self::{
    constants::{SslRequest, SslResponse},
    net_types::SslCallbackInfo,
};

#[cfg(feature = "ssl")]
pub use self::{
    constants::{SslCertExpiryOpt, SslCipherSuite},
    net_types::{SslSockConfig, SslSockOpts},
};

#[cfg(feature = "experimental-ecc")]
pub use self::{
    constants::EccRequestType,
    net_types::{EccInfo, EccPoint, EcdhInfo, EcdsaSignInfo},
};

#[cfg(feature = "experimental-ecc")]
pub(crate) use net_types::EccRequest;

pub use net_types::{
    AccessPoint, Credentials, HostName, MacAddress, ProvisioningInfo, S8Password, S8Username,
    SocketOptions, Ssid, TcpSockOpts, UdpSockOpts, WpaKey,
};

#[cfg(test)]
pub(crate) use net_types::Bssid;

#[cfg(feature = "wep")]
pub use net_types::WepKey;

#[cfg(feature = "ethernet")]
pub(crate) use net_types::EthernetRxInfo;

#[cfg(feature = "ethernet")]
pub use constants::MAX_TX_ETHERNET_PACKET_SIZE;

use registers::*;
use requests::*;
use responses::*;
pub use responses::{ConnectionInfo, FirmwareInfo, IPConf, ScanResult};

use core::net::{Ipv4Addr, SocketAddrV4};

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
    #[cfg(feature = "ssl")]
    Ssl(SslResponse),
}

/// HIF Request Group.
#[derive(Copy, Clone)]
enum HifRequest {
    Wifi(WifiRequest),
    Ip(IpCode),
    #[cfg(feature = "experimental-ota")]
    Ota(OtaRequest),
    #[cfg(feature = "ssl")]
    Ssl(SslRequest),
}

/// Implementation to convert `HifRequest` to `u8` value.
impl From<HifRequest> for u8 {
    fn from(v: HifRequest) -> Self {
        match v {
            HifRequest::Wifi(_) => 1,
            HifRequest::Ip(_) => 2,
            #[cfg(feature = "experimental-ota")]
            HifRequest::Ota(_) => 4,
            #[cfg(feature = "ssl")]
            HifRequest::Ssl(_) => 5,
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
            #[cfg(feature = "ssl")]
            5 => Self::Ssl(v[1].into()),
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
            #[cfg(feature = "ssl")]
            HifGroup::Ssl(_) => 5,
            _ => 0xFF,
        }
    }
}

fn hif_header_parse(hdr: [u8; HIF_HEADER_PACKET_SIZE]) -> Result<(HifGroup, u16), Error> {
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
/// Packet size for both HIF header requests and responses.
const HIF_HEADER_PACKET_SIZE: usize = 4;

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
    #[cfg(feature = "ssl")]
    fn on_ssl(
        &mut self,
        ssl_res: SslResponse,
        cipher_suite: Option<u32>,
        #[cfg(feature = "experimental-ecc")] ecc_req: Option<EccRequest>,
    );
    #[cfg(feature = "ethernet")]
    fn on_eth(&mut self, packet_size: u16, data_offset: u16, hif_address: u32);
}

// TODO: MAX_WAKERS should be a parameter on Manager<X: Xfer, const MAX_WAKERS: usize = 4>
#[cfg(feature = "async")]
const MAX_WAKERS: usize = 4; // Reasonable limit for concurrent async operations

pub struct Manager<X: Xfer> {
    // cached addresses
    not_a_reg_ctrl_4_dma: u32, // todo: make this dynamic/proper
    chip: ChipAccess<X>,
    #[cfg(feature = "async")]
    wakers: [Option<core::task::Waker>; MAX_WAKERS],
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
    mode: BootMode,
}
impl BootState {
    pub fn new(mode: BootMode) -> Self {
        Self {
            stage: BootStage::Start,
            loop_value: 0,
            mode,
        }
    }
}
impl<X: Xfer> Manager<X> {
    pub fn from_xfer(xfer: X) -> Self {
        Self {
            not_a_reg_ctrl_4_dma: 0x00,
            chip: ChipAccess::new(xfer),
            #[cfg(feature = "async")]
            wakers: core::array::from_fn(|_| None),
        }
    }
    #[cfg(test)]
    pub fn set_unit_test_mode(&mut self) {
        self.chip.set_unit_test_mode();
    }

    /// Register a waker to be called when hardware events are processed
    /// Returns error if waker array is full (too many concurrent operations)
    #[cfg(feature = "async")]
    pub fn register_waker(&mut self, waker: core::task::Waker) -> Result<(), crate::StackError> {
        // Find the first empty slot
        for slot in &mut self.wakers {
            if slot.is_none() {
                *slot = Some(waker);
                return Ok(());
            }
        }
        Err(crate::StackError::TooManyWakers)
    }

    /// Unregister a waker (called when operation completes)
    /// This removes any waker that matches the given waker
    #[cfg(feature = "async")]
    pub fn unregister_waker(&mut self, waker: &core::task::Waker) {
        for slot in &mut self.wakers {
            if let Some(ref stored_waker) = slot {
                if stored_waker.will_wake(waker) {
                    *slot = None;
                    break;
                }
            }
        }
    }

    /// Wake all registered wakers (called after dispatching events)
    #[cfg(feature = "async")]
    fn wake_all_wakers(&mut self) {
        for waker in self.wakers.iter_mut().flatten() {
            waker.wake_by_ref();
        }
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
        const FW_PATCH_MASK: u8 = 0x0F;
        const FW_MINOR_SHIFT: u8 = 0x04;

        let res = self.chip.single_reg_read(Regs::NmiRev.into())?;
        let unpack = res.to_le_bytes();
        Ok((
            Revision {
                major: unpack[1],
                minor: unpack[0] >> FW_MINOR_SHIFT,
                patch: unpack[0] & FW_PATCH_MASK,
            },
            Revision {
                major: unpack[3],
                minor: unpack[2] >> FW_MINOR_SHIFT,
                patch: unpack[2] & FW_PATCH_MASK,
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
        const BACKOFF_DELAY_USEC: u32 = 50_000; // 50 msec delay

        self.chip.single_reg_write(Regs::ChipReset.into(), 0)?;
        // back-off delay
        self.chip.delay_us(BACKOFF_DELAY_USEC);

        Ok(())
    }

    /// Halt the chip.
    ///
    /// # Returns
    ///
    /// * `()` - If the chip was successfully halted.
    /// * `Error` - If an error occurs while halting the chip.
    pub(crate) fn chip_halt(&mut self) -> Result<(), Error> {
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
        const WAKEUP_DELAY_USEC: u32 = 2000; // 2 msec delay
        const WAKEUP_OP_RETRIES: u8 = 4;

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

        let mut retries = WAKEUP_OP_RETRIES;
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
                let efuse =
                    self.chip.single_reg_read(Regs::EFuseRead.into())? & EFUSE_LOAD_DONE_BIT;
                if efuse != 0 {
                    state.stage = BootStage::Stage2;
                }
            }
            BootStage::Stage2 => {
                let host_wait =
                    self.chip.single_reg_read(Regs::WaitForHost.into())? & WAIT_FOR_HOST_BIT;
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
                const START_FIRMWARE: u32 = 0xef522f61;

                let driver_rev = 0x13521352; // todo
                self.chip
                    .single_reg_write(Regs::NmiState.into(), driver_rev)?;
                self.chip_id()?;
                // Write Boot Mode configuration.
                let conf = u32::from(state.mode) | NMI_GP1_PMU_EN_BIT;
                self.chip.single_reg_write(Regs::NmiGp1.into(), conf)?;
                let verify = self.chip.single_reg_read(Regs::NmiGp1.into())?;
                // Verify the configuration
                if verify == conf {
                    self.chip
                        .single_reg_write(Regs::BootRom.into(), START_FIRMWARE)?;
                    state.stage = BootStage::StageStartFirmware;
                    state.loop_value = 0;
                } else if state.loop_value >= MAX_LOOPS {
                    return Err(Error::FirmwareStart);
                }
            }
            BootStage::StageStartFirmware => {
                const FINISH_INIT: u32 = 0x02532636;
                const BACKOFF_DELAY_USEC: u32 = 2000; // 2 msec

                if state.loop_value >= MAX_LOOPS {
                    return Err(Error::FirmwareStart);
                }

                self.delay_us(BACKOFF_DELAY_USEC);
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
        const CLEAR_SPI_CONFIG: u32 = 0xFFFF_FF0F;
        const PACKET_SIZE_8K: u32 = 0x0000_0050;

        let mut conf = self.chip.single_reg_read(Regs::SpiConfig.into())?;
        conf &= CLEAR_SPI_CONFIG; // clear
        conf |= PACKET_SIZE_8K; // set to 8k packet size
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
        pinmux |= NMI_PIN_MUX0_EN_BIT;
        self.chip
            .single_reg_write(Regs::NmiPinMux0.into(), pinmux)?;
        trace!("Set pinmux to {:x}", pinmux);

        let mut int_enable = self.chip.single_reg_read(Regs::NmiIntrEnable.into())?;
        int_enable |= NMI_IRQ_EN_BIT;
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
        const LOW_WORD_MASK: u32 = 0x0000_FFFF;
        const FW_VER_PACKET_SIZE: usize = 40;

        let (_, address) = self.read_regs_from_otp_efuse()?;
        debug!("Got address {:#x}", address);
        let mod_address = (address & LOW_WORD_MASK) | EFUSE_OTP_MAC_OTA_BIT;
        let mut data = [0u8; FW_VER_PACKET_SIZE];
        debug!("Calculated address: {:#x}", mod_address);
        self.chip.dma_block_read(mod_address, data.as_mut_slice())?;
        Ok(data.into())
    }

    fn is_interrupt_pending(&mut self) -> Result<(bool, u32), Error> {
        let val = self.chip.single_reg_read(Regs::WifiHostRcvCtrl0.into())?;
        Ok((val & RCV_CTRL0_IRQ_BIT == RCV_CTRL0_IRQ_BIT, val))
    }
    fn clear_interrupt_pending(&mut self, ctrlreg: u32) -> Result<(), Error> {
        let setval = ctrlreg & !RCV_CTRL0_IRQ_BIT;
        self.chip
            .single_reg_write(Regs::WifiHostRcvCtrl0.into(), setval)
    }

    /// Reads the MAC address and firmware OTA version register addresses
    /// from the OTP (One-Time Programmable) eFuse memory.
    ///
    /// # Returns
    ///
    /// * `Ok((u32, u32))`:
    ///     - `u32`: MAC address register address.
    ///     - `u32`: Firmware OTA register address.
    /// * `Err(Error)` - If reading the eFuse memory fails.
    fn read_regs_from_otp_efuse(&mut self) -> Result<(u32, u32), Error> {
        // 4 bytes (u32) MAC + 4 bytes (u32) OTA FW.
        const OTP_EFUSE_PACKET_SIZE: usize = 8;
        /// Packet size of MAC/over-the-air (OTA) request
        const MAC_OTA_PACKET_SIZE: usize = 4;

        let read_addr = self.chip.single_reg_read(Regs::NmiGp2.into())?;
        let mod_read_add = read_addr | EFUSE_OTP_MAC_OTA_BIT;
        let mut data = [0u8; OTP_EFUSE_PACKET_SIZE];

        self.chip
            .dma_block_read(mod_read_add, data.as_mut_slice())?;

        let mut mac_efuse_mib = [0u8; MAC_OTA_PACKET_SIZE];
        mac_efuse_mib.copy_from_slice(&data[..MAC_OTA_PACKET_SIZE]);

        let mut firmware_ota_rev = [0u8; MAC_OTA_PACKET_SIZE];
        firmware_ota_rev.copy_from_slice(&data[MAC_OTA_PACKET_SIZE..]);

        Ok((
            u32::from_le_bytes(mac_efuse_mib),
            u32::from_le_bytes(firmware_ota_rev),
        ))
    }

    // #region read

    fn read_hif_header(&mut self, ctrlreg0: u32) -> Result<(HifGroup, u16, u32), Error> {
        let _size = (ctrlreg0 >> RCV_CTRL0_CLEAR_RX_BIT) & LOW_12_BIT_MASK;
        let address = self.chip.single_reg_read(Regs::WifiHostRcvCtrl1.into())?;
        let mut hif_header = [0u8; HIF_HEADER_PACKET_SIZE];
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
            .single_reg_write(Regs::WifiHostRcvCtrl0.into(), reg | RCV_CTRL0_CLEAR_RX_BIT)
    }
    // tstrRecvReply
    fn get_recv_reply<'b, const N: usize>(
        &mut self,
        address: u32,
        max_block: &'b mut [u8; N],
    ) -> Result<(Socket, SocketAddrV4, &'b [u8], SocketError), Error> {
        const TCP_RECV_REPLY_PACKET_SIZE: usize = 16;
        let mut result = [0xff; TCP_RECV_REPLY_PACKET_SIZE];

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
        const NMI_STATE_REQ_DATA: u8 = 0x80;

        // Write NMI state
        let mut state: u32 = 0;
        state |= gid as u32;
        state |= if req_data {
            ((op | NMI_STATE_REQ_DATA) as u32) << NMI_STATE_OP_BIT
        } else {
            (op as u32) << NMI_STATE_OP_BIT
        };
        state |= (len as u32) << NMI_STATE_LEN_BIT;
        self.chip.single_reg_write(Regs::NmiState.into(), state)?;
        // Set RCV_CTRL_2 bit 1
        self.chip
            .single_reg_write(Regs::WifiHostRcvCtrl2.into(), RCV_CTRL2_BIT_1)?;

        // Wait for bit 1 in RCV_CTRL_2 to clear, with timeout
        let mut retries = HIF_SEND_RETRIES;
        let mut res;
        loop {
            res = self.chip.single_reg_read(Regs::WifiHostRcvCtrl2.into())?;
            res &= RCV_CTRL2_BIT_1;
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
            #[cfg(feature = "ssl")]
            HifRequest::Ssl(opcode) => opcode.into(),
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
        let val = (addr << RCV_CTRL3_ADDR_SHIFT) | RCV_CTRL3_DONE_BIT;
        self.chip.single_reg_write(
            Regs::WifiHostRcvCtrl3.into(),
            // dma_addr come from ctrl4
            //reg = dma_addr << 2; reg |= NBIT1;
            val,
        )
    }

    /// Determines the appropriate SSL `IpCode` variant for the given socket.
    ///
    /// If the provided `socket` has SSL enabled, the function converts the
    /// *base* `IpCode` (e.g., `IpCode::Connect`) into its SSL counterpart
    /// (e.g., `IpCode::SslConnect`). When SSL is not active, the original `base`
    /// value is returned unchanged.
    ///
    /// # Arguments
    ///
    /// * `_socket` – A reference to the `Socket` whose SSL configuration will be inspected.
    /// * `base` – The `IpCode` command to convert to its SSL counterpart.
    ///
    /// # Returns
    ///
    /// * An `IpCode::Ssl...` variant when SSL is enabled.
    /// * The original `base` value when SSL is disabled.
    fn get_ssl_ip_code(&mut self, _socket: &Socket, base: IpCode) -> IpCode {
        #[cfg(feature = "ssl")]
        if (_socket.get_ssl_cfg() & u8::from(SslSockConfig::EnableSSL))
            == u8::from(SslSockConfig::EnableSSL)
        {
            return match base {
                IpCode::Connect => IpCode::SslConnect,
                IpCode::Send => IpCode::SslSend,
                IpCode::Recv => IpCode::SslRecv,
                IpCode::Bind => IpCode::SslBind,
                IpCode::Close => IpCode::SslClose,
                _ => base,
            };
        }
        base
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
        const GET_HOSTNAME_PACKET_SIZE: usize = 64;

        let mut buffer = [0x0u8; GET_HOSTNAME_PACKET_SIZE];
        let req = write_gethostbyname_req(host, &mut buffer)?;

        self.write_hif_header(HifRequest::Ip(IpCode::DnsResolve), req, false)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, req)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a request to bind the socket to the specified IPv4 address and port.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket to bind.
    /// * `address` - The local IPv4 address and port to bind the socket to.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the bind request was successfully sent.
    /// * `Err(Error)` - If an error occurred while sending the bind request.
    pub fn send_bind(&mut self, socket: Socket, address: SocketAddrV4) -> Result<(), Error> {
        let req = write_bind_req(socket, address)?;
        let cmd = self.get_ssl_ip_code(&socket, IpCode::Bind);

        self.write_hif_header(HifRequest::Ip(cmd), &req, false)?;
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

    /// Sends a request to initiate a socket connection to the specified IPv4 address.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket to use for connecting to the server.
    /// * `address` - The IPv4 address and port to connect to.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the connection request was successfully sent.
    /// * `Err(Error)` - If an error occurred while sending the connection request.
    pub fn send_socket_connect(
        &mut self,
        socket: Socket,
        address: SocketAddrV4,
    ) -> Result<(), Error> {
        let req = write_connect_req(socket, AF_INET, address, socket.get_ssl_cfg())?;
        let cmd = self.get_ssl_ip_code(&socket, IpCode::Connect);

        self.write_hif_header(HifRequest::Ip(cmd), &req, false)?;
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
        let req = write_sendto_req(socket, AF_INET, address, data.len())?;
        self.write_hif_header(HifRequest::Ip(IpCode::SendTo), &req, true)?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        self.chip.dma_block_write(
            self.not_a_reg_ctrl_4_dma + UDP_TX_PACKET_OFFSET as u32,
            data,
        )?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a TCP send request.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket through which the data will be sent.
    /// * `data` - A byte slice containing the data to send.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the data was successfully sent.
    /// * `Err(Error)` - If an error occurred during the send operation.
    pub fn send_send(&mut self, socket: Socket, data: &[u8]) -> Result<(), Error> {
        const TCP_IP_HEADER_LENGTH: usize = 40;
        const TCP_TX_PACKET_OFFSET: usize = IP_PACKET_OFFSET + TCP_IP_HEADER_LENGTH;

        // todo: offset depends on UDP or TCP
        let req = write_sendto_req(
            socket,
            AF_INET,
            SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0),
            data.len(),
        )?;

        let cmd = self.get_ssl_ip_code(&socket, IpCode::Send);

        let offset = {
            #[cfg(feature = "ssl")]
            {
                if matches!(cmd, IpCode::SslSend) {
                    // Offset received from connect command response.
                    let data_offset = socket.get_ssl_data_offset() as usize;
                    if data_offset == 0 {
                        error!("Attempted to send on an SSL socket with an invalid data offset.");
                        return Err(Error::Failed);
                    }
                    data_offset
                } else {
                    TCP_TX_PACKET_OFFSET
                }
            }

            #[cfg(not(feature = "ssl"))]
            {
                TCP_TX_PACKET_OFFSET
            }
        };

        self.write_hif_header_impl(HifRequest::Ip(cmd), &req, true, Some((data.len(), offset)))?;
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;
        // The offset already includes the HIF_HEADER_OFFSET, so it is not added here.
        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + offset as u32, data)?;
        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a request to receive data from the specified socket, with a timeout.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket from which to receive data.
    /// * `timeout` - The timeout duration in milliseconds.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the receive request was successfully sent.
    /// * `Err(Error)` - If an error occurred while sending the receive request.
    pub fn send_recv(&mut self, socket: Socket, timeout: u32) -> Result<(), Error> {
        let req = write_recv_req(socket, timeout)?;
        let cmd = self.get_ssl_ip_code(&socket, IpCode::Recv);

        self.write_hif_header(HifRequest::Ip(cmd), &req, false)?;
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

    /// Sends a request to close the specified socket.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket to close.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the close request was successfully sent.
    /// * `Err(Error)` - If an error occurred while sending the request.
    pub fn send_close(&mut self, socket: Socket) -> Result<(), Error> {
        let req = write_close_req(socket)?;
        let cmd = self.get_ssl_ip_code(&socket, IpCode::Close);

        self.write_hif_header(HifRequest::Ip(cmd), &req, false)?;
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
    #[cfg(feature = "ssl")]
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
    /// * `server_url` - Server URL from where firmware image will be downloaded.
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
        const LOW_20_BIT_MASK: u32 = 0x000F_FFFF;
        const DATA_SIZE_OFFSET: u32 = 0x08;
        // 0x04 CMD length, 4 bits are high.
        const CMD_SIZE_IN_BITS: u32 = 0x0F;

        if data_size > FLASH_PAGE_SIZE {
            return Err(Error::ExceedsFlashPageSize);
        }

        let cmd = {
            let b = flash_addr.to_be_bytes();
            [FlashOpCode::PageProgram.into(), b[1], b[2], b[3]]
        };

        // Flash CMD length: 0x04 + MASK: 0x80
        let cmd_count: u32 = cmd.len() as u32 + FLASH_CMD_CNT_MASK;

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), u32::from_le_bytes(cmd))?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), CMD_SIZE_IN_BITS)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), Regs::FlashSharedMemory.into())?;

        // Mask data_size to 20 bits, shift to high bytes, and set 0x84 as the low byte
        let size = cmd_count | ((data_size as u32 & LOW_20_BIT_MASK) << DATA_SIZE_OFFSET);

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
        // 0x01 CMD length, 1 bits are high.
        const CMD_SIZE_IN_BITS: u32 = 0x01;
        // Size of data requested from flash.
        const DATA_SIZE_REQ: u32 = 0x04;
        // Flash CMD bytes: 0x01 + MASK: 0x80
        const CMD_COUNT_MASK: u32 = 1 | FLASH_CMD_CNT_MASK;

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), DATA_SIZE_REQ)?;
        self.chip.single_reg_write(
            Regs::FlashBuffer1.into(),
            FlashOpCode::ReadStatusRegister as u32,
        )?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), CMD_SIZE_IN_BITS)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), FLASH_DUMMY_VALUE)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), CMD_COUNT_MASK)?;

        // read transfer complete register.
        self.check_flash_tx_complete()?;

        let res = self.chip.single_reg_read(FLASH_DUMMY_VALUE)?;
        Ok(res as u8)
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
        // 0x05 CMD length, 5 bits are high.
        const CMD_SIZE_IN_BITS: u32 = 0x1F;
        // Last byte of command
        const CMD_LAST_BYTE: u32 = 0xA5;

        let cmd = {
            let b = flash_addr.to_be_bytes();
            [FlashOpCode::FastRead.into(), b[1], b[2], b[3]]
        };

        // Flash CMD bytes: 0x05 + MASK: 0x80
        let cmd_count: u32 = (cmd.len() as u32 + 1) | FLASH_CMD_CNT_MASK;

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), data_size as u32)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), u32::from_le_bytes(cmd))?;
        self.chip
            .single_reg_write(Regs::FlashBuffer2.into(), CMD_LAST_BYTE)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), CMD_SIZE_IN_BITS)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), Regs::FlashSharedMemory.into())?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), cmd_count)?;
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
        // 0x04 CMD length, 4 bits are high.
        const CMD_SIZE_IN_BITS: u32 = 0x0F;

        let cmd = {
            let b = flash_addr.to_be_bytes();
            [FlashOpCode::SectorErase.into(), b[1], b[2], b[3]]
        };

        // Flash CMD bytes: 0x04 + MASK: 0x80
        let cmd_count: u32 = (cmd.len() as u32) | FLASH_CMD_CNT_MASK;

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip
            .single_reg_write(Regs::FlashBuffer1.into(), u32::from_le_bytes(cmd))?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), CMD_SIZE_IN_BITS)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), 0)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), cmd_count)?;

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
        // 0x01 CMD length, 1 bits are high.
        const CMD_SIZE_IN_BITS: u32 = 0x01;
        // Flash CMD bytes: 0x01 + MASK: 0x80
        const CMD_COUNT_MASK: u32 = 1 | FLASH_CMD_CNT_MASK;

        let cmd = if enable {
            FlashOpCode::WriteEnable as u32
        } else {
            FlashOpCode::WriteDisable as u32
        };

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip.single_reg_write(Regs::FlashBuffer1.into(), cmd)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), CMD_SIZE_IN_BITS)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), 0x00)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), CMD_COUNT_MASK)?;
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

        while (res & FLASH_READ_STATUS_BIT) != 0 {
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
        // 0x01 CMD length, 1 bits are high.
        const CMD_SIZE_IN_BITS: u32 = 0x01;
        // Flash CMD bytes: 0x01 + MASK: 0x80
        const CMD_COUNT_MASK: u32 = 1 | FLASH_CMD_CNT_MASK;
        // Size of data requested from flash.
        const DATA_SIZE_REQ: u32 = 0x04;

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), DATA_SIZE_REQ)?;
        self.chip.single_reg_write(
            Regs::FlashBuffer1.into(),
            FlashOpCode::ReadIdentification as u32,
        )?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), CMD_SIZE_IN_BITS)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), FLASH_DUMMY_VALUE)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), CMD_COUNT_MASK)?;
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
        // 0x01 CMD length, so 1 bits are high.
        const CMD_SIZE_IN_BITS: u32 = 0x01;
        // Flash CMD bytes: 0x01 + MASK: 0x80
        const CMD_COUNT_MASK: u32 = 1 | FLASH_CMD_CNT_MASK;

        let cmd: u32 = if enable {
            FlashOpCode::EnterPowerSleep as u32
        } else {
            FlashOpCode::ExitPowerSleep as u32
        };

        self.chip
            .single_reg_write(Regs::FlashDataCount.into(), 0x00)?;
        self.chip.single_reg_write(Regs::FlashBuffer1.into(), cmd)?;
        self.chip
            .single_reg_write(Regs::FlashBufferDirectory.into(), CMD_SIZE_IN_BITS)?;
        self.chip
            .single_reg_write(Regs::FlashDmaAddress.into(), 0)?;
        self.chip
            .single_reg_write(Regs::FlashCommandCount.into(), CMD_COUNT_MASK)?;
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
        let mut val = self.chip.single_reg_read(Regs::FlashPinMux.into())?;

        val &= !((FlashPinMux::GpioPins as u32) << FlashPinMux::Offset as u32);

        val |= if enable {
            (FlashPinMux::Enable as u32) << FlashPinMux::Offset as u32
        } else {
            (FlashPinMux::Disable as u32) << FlashPinMux::Offset as u32
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

    /// Sends a request to create an SSL socket.
    ///
    /// # Arguments
    ///
    /// * `socket` - The socket to enable SSL on.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the request was successfully sent.
    /// * `Err(Error)` - If an error occurred while preparing or sending the request.
    #[cfg(feature = "ssl")]
    pub(crate) fn send_ssl_sock_create(&mut self, socket: Socket) -> Result<(), Error> {
        let req = [socket.v, 0, 0, 0];

        self.write_hif_header(HifRequest::Ip(IpCode::SslCreate), &req, false)?;

        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;

        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends a request to configure the SSL certificate expiry option.
    ///
    /// # Arguments
    ///
    /// * `opt` - The SSL certificate expiry option to configure.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the request was successfully sent.
    /// * `Err(Error)` - If an error occurred while preparing or sending the request.
    #[cfg(feature = "ssl")]
    pub(crate) fn send_ssl_cert_expiry(&mut self, opt: SslCertExpiryOpt) -> Result<(), Error> {
        let req = u32::to_le_bytes(opt.into());

        self.write_hif_header(HifRequest::Ip(IpCode::SslExpCheck), &req, false)?;

        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;

        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Sends an ECC response to the module.
    ///
    /// # Arguments
    ///
    /// * `ecc_info` - A reference to the ECC operation information.
    /// * `ecdh_info` - An optional reference to the ECDH operation information.
    /// * `resp_buffer` - A buffer containing the ECC response data to send.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the response was successfully sent.
    /// * `Err(Error)` - If an error occurred while preparing or sending the response.
    #[cfg(feature = "experimental-ecc")]
    pub(crate) fn send_ecc_resp(
        &mut self,
        ecc_info: &EccInfo,
        ecdh_info: Option<&EcdhInfo>,
        resp_buffer: &[u8],
    ) -> Result<(), Error> {
        let req = write_ssl_ecc_resp(ecc_info, ecdh_info)?;

        self.write_hif_header_impl(
            HifRequest::Ssl(SslRequest::SendEccResponse),
            &req,
            true,
            Some((resp_buffer.len(), req.len())),
        )?;

        // write the control packet
        let mut reg = self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32;
        self.chip.dma_block_write(reg, &req)?;

        // write the data packet
        reg += req.len() as u32;
        self.chip.dma_block_write(reg, resp_buffer)?;

        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Reads ECC information (curve type, hash algorithm, and signature) from the WINC module.
    ///
    /// # Arguments
    ///
    /// * `address` - The register address to start reading from.
    /// * `resp_buff` - A mutable buffer to store the response data.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the data was successfully read.
    /// * `Err(Error)` - If an error occurred while reading the data.
    #[cfg(feature = "experimental-ecc")]
    pub(crate) fn read_ecc_info(
        &mut self,
        address: u32,
        resp_buff: &mut [u8],
    ) -> Result<(), Error> {
        let result = self
            .chip
            .dma_block_read(address + HIF_HEADER_OFFSET as u32, resp_buff);

        // Set the RX done if error occurs, this will clear the information
        // from the module.
        if result.is_err() {
            error!("Failed to read SSL info at address {:x}", address);
            error!("Sending request to stop reading from the module.");
            let reg = self.chip.single_reg_read(Regs::WifiHostRcvCtrl0.into())?;
            self.chip
                .single_reg_write(Regs::WifiHostRcvCtrl0.into(), reg | RCV_CTRL0_CLEAR_RX_BIT)?;

            return Err(Error::ReadError);
        }

        Ok(())
    }

    /// Sends a request to stop reading ECC information from the module.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If ECC information reading was successfully disabled.
    /// * `Err(Error)` - If an error occurred while updating the module state.
    #[cfg(feature = "experimental-ecc")]
    pub(crate) fn send_ecc_read_complete(&mut self) -> Result<(), Error> {
        let reg = self.chip.single_reg_read(Regs::WifiHostRcvCtrl0.into())?;
        self.chip
            .single_reg_write(Regs::WifiHostRcvCtrl0.into(), reg | RCV_CTRL0_CLEAR_RX_BIT)
    }

    /// Sends a request to set the desired SSL cipher suites.
    ///
    /// # Arguments
    ///
    /// * `cipher_bitmap` - A bitmask (`u32`) representing the cipher suites to enable.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the request was successfully sent.
    /// * `Err(Error)` - If an error occurred while updating the module state.
    #[cfg(feature = "ssl")]
    pub(crate) fn send_ssl_set_cipher_suite(&mut self, cipher_bitmap: u32) -> Result<(), Error> {
        let req = cipher_bitmap.to_le_bytes();

        self.write_hif_header(HifRequest::Ssl(SslRequest::SetCipherSuites), &req, false)?;

        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;

        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Reads the MAC address from the OTP (One-Time Programmable) eFuse memory.
    ///
    /// # Returns
    ///
    /// * `Ok(MacAddress)` - The MAC address successfully read from the eFuse.
    /// * `Err(Error)` - If reading the mac address from eFuse fails.
    pub(crate) fn read_otp_mac_address(
        &mut self,
        #[cfg(test)] test_hook: bool,
    ) -> Result<MacAddress, Error> {
        const MAC_ADDR_OFFSET: u32 = 16;
        const HIGH_WORD_MASK: u32 = 0xFFFF_0000;

        let mac: u32 = {
            #[cfg(not(test))]
            {
                self.read_regs_from_otp_efuse()?.0
            }

            #[cfg(test)]
            {
                if test_hook {
                    HIGH_WORD_MASK
                } else {
                    0x0000_0000
                }
            }
        };

        let reg = match mac & HIGH_WORD_MASK {
            0 => return Err(Error::BufferReadError),
            r => (r >> MAC_ADDR_OFFSET) | EFUSE_OTP_MAC_OTA_BIT,
        };

        let mut mac_address = MacAddress::empty();
        self.chip.dma_block_read(reg, mac_address.as_mut_slice())?;

        Ok(mac_address)
    }

    /// Sends an Ethernet packet with a maximum size of
    /// `MAX_TX_ETHERNET_PACKET_SIZE` (65,501) to the module.
    ///
    /// # Arguments
    ///
    /// * `net_pkt` - The Ethernet packet to send.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the packet was successfully sent.
    /// * `Err(Error)` - If an error occurred while sending the packet.
    #[cfg(feature = "ethernet")]
    pub(crate) fn send_ethernet_packet(&mut self, net_pkt: &[u8]) -> Result<(), Error> {
        if net_pkt.is_empty() || (net_pkt.len() > MAX_TX_ETHERNET_PACKET_SIZE) {
            return Err(Error::BufferError);
        }
        let req = write_send_net_pkt_req(net_pkt.len() as u16, ETHERNET_HEADER_LENGTH as u16)?;

        self.write_hif_header_impl(
            HifRequest::Wifi(WifiRequest::SendEthernetPacket),
            &req,
            true,
            Some((net_pkt.len(), ETHERNET_HEADER_OFFSET - HIF_HEADER_OFFSET)),
        )?;

        self.chip
            .dma_block_write(self.not_a_reg_ctrl_4_dma + HIF_HEADER_OFFSET as u32, &req)?;

        self.chip.dma_block_write(
            self.not_a_reg_ctrl_4_dma + ETHERNET_HEADER_OFFSET as u32,
            net_pkt,
        )?;

        self.write_ctrl3(self.not_a_reg_ctrl_4_dma)
    }

    /// Receives an Ethernet packet from the module.
    ///
    /// # Arguments
    ///
    /// * `address` - The starting HIF address from which the Ethernet packet will be read.
    /// * `buffer` - A mutable buffer where the received Ethernet packet will be stored.
    /// * `rx_done` - Indicates whether the RX_DONE flag should be written after the packet is read.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the packet is successfully read from the module.
    /// * `Err(Error)` - If an error occurs while receiving the packet.
    #[cfg(feature = "ethernet")]
    pub(crate) fn recv_ethernet_packet(
        &mut self,
        address: u32,
        buffer: &mut [u8],
        rx_done: bool,
    ) -> Result<(), Error> {
        self.chip.dma_block_read(address, buffer)?;
        // clear RX if no more data is available to read.
        if rx_done {
            let reg = self.chip.single_reg_read(Regs::WifiHostRcvCtrl0.into())?;
            self.chip
                .single_reg_write(Regs::WifiHostRcvCtrl0.into(), reg | RCV_CTRL0_CLEAR_RX_BIT)?;
        }

        Ok(())
    }

    // #endregion write
}

#[cfg(test)]
mod tests {
    use super::*;
    use constants::PRNG_PACKET_SIZE;

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
                2, 0, // address family
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
    fn test_connect() {
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

        let slice = &buff[DATA_OFFSET..DATA_OFFSET + ENABLE_AP_REQ_PACKET_SIZE];

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

    #[test]
    fn test_hif_header_exceeded_len() {
        let mut buff = [0u8; 73];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);

        let req = HifRequest::Wifi(WifiRequest::Connect);
        let payload = [0u8; 65535];
        let data_pkt = Some((65535, 128));

        let res = mgr.write_hif_header_impl(req, &payload, true, data_pkt);

        assert_eq!(res, Err(Error::BufferError));
    }

    #[test]
    fn test_get_ip_code_no_ssl() {
        let mut buff = [0u8; 10];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);
        let sock = Socket::new(1, 1);
        let ip_code = IpCode::Connect;

        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);

        assert_eq!(new_ip_code, ip_code);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_get_ip_code_ssl_enabled() {
        let mut buff = [0u8; 10];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);
        let mut sock = Socket::new(1, 1);
        let ip_code = IpCode::Connect;

        sock.set_ssl_cfg(SslSockConfig::EnableSSL.into(), true);
        sock.set_ssl_cfg(SslSockConfig::BypassX509Verification.into(), true);

        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);

        assert_eq!(new_ip_code, IpCode::SslConnect);

        let ip_code = IpCode::Bind;
        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);

        assert_eq!(new_ip_code, IpCode::SslBind);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_get_ip_code_ssl_not_enabled() {
        let mut buff = [0u8; 10];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);
        let mut sock = Socket::new(1, 1);
        let ip_code = IpCode::Send;

        sock.set_ssl_cfg(SslSockConfig::BypassX509Verification.into(), true);

        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);

        assert_eq!(new_ip_code, ip_code);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_get_ip_code_ssl_not_applicable() {
        let mut buff = [0u8; 10];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);
        let mut sock = Socket::new(1, 1);
        let ip_code = IpCode::RecvFrom;

        sock.set_ssl_cfg(SslSockConfig::EnableSSL.into(), true);

        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);

        assert_eq!(new_ip_code, ip_code);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_get_ip_code_verify_ssl_opts() {
        let mut buff = [0u8; 10];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);
        let mut sock = Socket::new(1, 1);

        sock.set_ssl_cfg(SslSockConfig::EnableSSL.into(), true);

        // Connect + Send is verified from other tests.

        // Bind
        let ip_code = IpCode::Bind;
        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);
        assert_eq!(new_ip_code, IpCode::SslBind);

        // Recv
        let ip_code = IpCode::Recv;
        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);
        assert_eq!(new_ip_code, IpCode::SslRecv);

        // Close
        let ip_code = IpCode::Close;
        let new_ip_code = mgr.get_ssl_ip_code(&sock, ip_code);
        assert_eq!(new_ip_code, IpCode::SslClose);
    }

    #[cfg(feature = "ssl")]
    #[test]
    fn test_ssl_send_success() {
        let mut buff = [0u8; 120];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);

        let mut sock = Socket::new(7, 522);
        sock.set_ssl_cfg(SslSockConfig::EnableSSL.into(), true);
        sock.set_ssl_data_offset(100);

        assert_eq!(mgr.send_send(sock, &[42]), Ok(()));
        assert_eq!(buff[CMD_OFFSET], u8::from(IpCode::SslSend));
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

    #[cfg(feature = "ssl")]
    #[test]
    fn test_ssl_send_offset_failed() {
        let mut buff = [0u8; 10];
        let mut writer = buff.as_mut_slice();
        let mut mgr = make_manager(&mut writer);

        let mut sock = Socket::new(7, 522);
        sock.set_ssl_cfg(SslSockConfig::EnableSSL.into(), true);

        assert_eq!(mgr.send_send(sock, &[42]), Err(Error::Failed));
    }
}
