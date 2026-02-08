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

/// Enable read/write access to Cortus registers.
pub(super) const INTR_REG_RW_EN_BIT: u8 = 0x80;
/// Read OTA firmware version and MAC address from EFUSE OTP.
pub(super) const EFUSE_OTP_MAC_OTA_BIT: u32 = 0x30000;
/// Halt the Cortus processor.
pub(super) const HALT_BIT: u32 = 0x01;
/// Reset the Cortus processor.
pub(super) const RESET_BIT: u32 = 0x400;
/// Wake up the Cortus processor.
pub(super) const WAKEUP_BIT: u32 = 0x01;
/// Check the Cortus clock when waking from power-off or sleep.
pub(super) const WAKEUP_CLK_BIT: u32 = 0x02;
/// Check Cortus clock is enabled after waking up from sleep or power-down.
pub(super) const CLK_EN_BIT: u32 = 0x04;
/// EFUSE is ready to be read.
pub(super) const EFUSE_LOAD_DONE_BIT: u32 = 0x8000_0000;
/// Check whether the `Wait for Host` register is ready.
pub(super) const WAIT_FOR_HOST_BIT: u32 = 0x01;
/// Enable the Power Management Unit (PMU) of NMI/WINC (Network Machine Interface).
/// Includes reserved bits and the ENABLE_PMU bit.
pub(super) const NMI_GP1_PMU_EN_BIT: u32 = 0x102;
/// Enable pin muxing for NMI.
pub(super) const NMI_PIN_MUX0_EN_BIT: u32 = 0x100;
/// Enable IRQ for NMI.
pub(super) const NMI_IRQ_EN_BIT: u32 = 0x10000;
/// Check whether an IRQ has been received.
pub(super) const RCV_CTRL0_IRQ_BIT: u32 = 0x01;
/// Write the data/control packet length to the NMI state register.
pub(super) const NMI_STATE_LEN_BIT: u32 = 0x10;
/// Write the operation ID to the NMI state register.
pub(super) const NMI_STATE_OP_BIT: u32 = 0x08;
/// Wait for NMI to be ready to receive a new HIF packet from the host.
pub(super) const RCV_CTRL2_BIT_1: u32 = 0x02;
/// Host has finished reading data from NMI.
pub(super) const RCV_CTRL0_CLEAR_RX_BIT: u32 = 0x02;
/// Last cortus register that can be written.
pub(super) const CORTUS_WRITE_MAX_REG: u32 = 0x30;
/// Last Cortus register that can be read.
pub(super) const CORTUS_READ_MAX_REG: u32 = 0xFF;
/// Bit position of the address field in the RCV_CTRL3 register.
pub(super) const RCV_CTRL3_ADDR_SHIFT: u32 = 2;
/// Host has finished sending the HIF packet to NMI.
pub(super) const RCV_CTRL3_DONE_BIT: u32 = 0x02;
/// Mask for sending the command count to access flash memory.
#[cfg(feature = "flash-rw")]
pub(super) const FLASH_CMD_CNT_MASK: u32 = 0x80;
/// Mask for extracting the lower 12 bits.
pub(crate) const LOW_12_BIT_MASK: u32 = 0x0000_0FFF;
/// Flash memory read-status bit.
#[cfg(feature = "flash-rw")]
pub(crate) const FLASH_READ_STATUS_BIT: u8 = 0x01;
/// Flash memory read size info bit.
#[cfg(feature = "flash-rw")]
pub(crate) const FLASH_SIZE_INFO_SHIFT: u32 = 0x10;

/// WINC(NMI) and Cortus Register
#[repr(u32)]
pub(super) enum Regs {
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
    _FlashMsbControl = 0x10220,
    #[cfg(feature = "flash-rw")]
    _FlashTransmitControl = 0x10224,
    #[cfg(feature = "flash-rw")]
    FlashSharedMemory = 0xd0000,
    #[cfg(feature = "flash-rw")]
    FlashPinMux = 0x1410,
}

/// Implementation to convert `Regs` to `u32`.
impl From<Regs> for u32 {
    fn from(val: Regs) -> Self {
        val as u32
    }
}

/// Flash operation codes (compatible with MX25L6465E).
#[repr(u8)]
#[cfg(feature = "flash-rw")]
pub(super) enum FlashOpCode {
    PageProgram = 0x02,
    FastRead = 0x0b,
    ReadStatusRegister = 0x05,
    SectorErase = 0x20,
    WriteEnable = 0x06,
    WriteDisable = 0x04,
    ReadIdentification = 0x9F,
    EnterPowerSleep = 0xB9,
    ExitPowerSleep = 0xAB,
}

/// Implementation to convert `FlashOpCode` to `u8`.
#[cfg(feature = "flash-rw")]
impl From<FlashOpCode> for u8 {
    fn from(val: FlashOpCode) -> Self {
        val as u8
    }
}

/// Flash pin-mux masks.
#[repr(u32)]
#[cfg(feature = "flash-rw")]
pub(super) enum FlashPinMux {
    GpioPins = 0x7777, // GPIO15/16/17/18
    Enable = 0x1111,
    Disable = 0x0010,
    Offset = 0x0C,
}

/// Implementation to convert `FlashPinMux` to `u32`.
#[cfg(feature = "flash-rw")]
impl From<FlashPinMux> for u32 {
    fn from(val: FlashPinMux) -> Self {
        val as u32
    }
}
