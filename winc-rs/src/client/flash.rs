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

use super::{StackError, WincClient, Xfer};
use crate::manager::FLASH_PAGE_SIZE;
use crate::{error, info};

/// Block Size of flash memory.
const FLASH_BLOCK_SIZE: usize = 32 * 1024;
/// Supported chip rev for low power mode.
const CHIP_REV_LP: u32 = 0x3A0;

impl<X: Xfer> WincClient<'_, X> {
    /// Enables or disables read/write access to the flash.
    ///
    /// # Arguments
    ///
    /// * `enable` – `true` to enable access, `false` to disable.
    ///
    /// # Returns
    ///
    /// * `()` – Flash access was successfully enabled or disabled.
    /// * `StackError` – If an error occurs while enabling or disabling access to the flash.
    pub fn set_flash_access(&mut self, enable: bool) -> Result<(), StackError> {
        let mut chip_id = self.manager.chip_id()?;
        chip_id &= 0x00000fff;

        if chip_id >= CHIP_REV_LP {
            if enable {
                // enable pinmux on flash.
                self.manager.send_flash_pin_mux(true)?;
                // exit the low power mode.
                self.manager.send_flash_low_power_mode(false)?;
            } else {
                // disable pinmux on flash to minimize current leakage.
                self.manager.send_flash_pin_mux(false)?;
                // enter low power mode.
                self.manager.send_flash_low_power_mode(true)?;
            }
        }

        Ok(())
    }

    /// Reads data from flash memory.
    ///
    /// # Arguments
    ///
    /// * `addr` – The flash memory address to read from.
    /// * `buffer` – A mutable buffer where the read data will be stored.
    ///
    /// # Returns
    ///
    /// * `()` – Data was successfully read from flash memory.
    /// * `StackError` – If an error occurs while reading data from the flash.
    pub fn flash_read(&mut self, addr: u32, buffer: &mut [u8]) -> Result<(), StackError> {
        if buffer.is_empty() {
            return Err(StackError::InvalidParameters);
        }

        let mut offset: usize = 0;
        let mut flash_addr = addr;

        loop {
            let to_recv = (buffer[offset..]).len().min(FLASH_BLOCK_SIZE);
            // read the data
            self.manager
                .send_flash_read(flash_addr, &mut buffer[offset..offset + to_recv])?;
            offset += to_recv;
            // check if all data is read.
            if offset >= buffer.len() {
                return Ok(());
            } else {
                flash_addr += to_recv as u32;
            }
        }
    }

    /// Writes data to flash memory.
    ///
    /// # Arguments
    ///
    /// * `addr` – The flash memory address to start writing at.
    /// * `data` – The data to be written.
    ///
    /// # Returns
    ///
    /// * `()` – Data was successfully written to flash memory.
    /// * `StackError` – If an error occurs while writing data to the flash.
    pub fn flash_write(&mut self, addr: u32, data: &[u8]) -> Result<(), StackError> {
        if data.is_empty() {
            return Err(StackError::InvalidParameters);
        }

        let page_offset = addr as usize % FLASH_PAGE_SIZE; // offset on current page of flash memory.
        let mut offset: usize = 0; // buffer offset.
        let mut flash_addr = addr; // current flash memory address.

        if page_offset > 0 {
            let word_size = FLASH_PAGE_SIZE - page_offset;
            let valid_size = data.len().min(word_size);
            self.manager
                .send_flash_write(flash_addr, &data[..valid_size])?;
            // check if all bytes are written.
            if data.len() <= word_size {
                return Ok(());
            }
            // Increament the buffer and flash address by bytes written.
            offset += word_size;
            flash_addr += word_size as u32;
        }

        while offset < data.len() {
            let word_size = data[offset..].len().min(FLASH_PAGE_SIZE);

            self.manager
                .send_flash_write(flash_addr, &data[offset..offset + word_size])?;

            // Increament the buffer and flash address by bytes written.
            offset += word_size;
            flash_addr += word_size as u32;
        }

        Ok(())
    }

    /// Erases a region of flash memory.
    ///
    /// # Arguments
    ///
    /// * `addr` – The address in flash memory to erase.
    /// * `size` – The size of the region to erase, in bytes. Must be aligned to the flash sector or block size.
    ///
    /// # Returns
    ///
    /// * `()` – Flash memory was successfully erased.
    /// * `StackError` – If an error occurs while erasing the flash memory.
    pub fn flash_erase(&mut self, addr: u32, size: u32) -> Result<(), StackError> {
        if size == 0 {
            return Ok(());
        }

        const SECTOR_SIZE: u32 = (16 * FLASH_PAGE_SIZE) as u32; // 4 KiB
        let end = addr
            .checked_add(size)
            .ok_or(StackError::InvalidParameters)?;
        let mut flash_addr: u32 = addr;

        while flash_addr < end {
            let mut retries: u8 = 3;

            // enable write access
            self.manager.send_flash_write_access(true)?;

            let _ = self.manager.send_flash_read_status_register()?;
            self.manager.send_flash_erase_sector(flash_addr + 10)?; // Wifi101 adds 10 in the flash address.

            let mut val = self.manager.send_flash_read_status_register()?;

            while (val & 0x01) != 0 {
                if retries == 0 {
                    error!(
                        "Erasing flash sector failed due to invalid flash status register value."
                    );
                    return Err(StackError::GeneralTimeout);
                }
                retries -= 1;
                val = self.manager.send_flash_read_status_register()?;
            }

            flash_addr = flash_addr.saturating_add(SECTOR_SIZE);
        }

        // disable write access
        self.manager.send_flash_write_access(false)?;

        Ok(())
    }

    /// Returns the size of the flash memory.
    ///
    /// # Returns
    ///
    /// * `u32` – The size of the flash memory in Mega Bits.
    /// * `StackError` – If an error occurs while retrieving the size of the flash memory.
    pub fn flash_get_size(&mut self) -> Result<u32, StackError> {
        const FLASH_ID_SIZE_OFFSET: u32 = 0x11;
        let id = self.manager.send_flash_read_id()?;

        if id == 0xffffffff {
            error!("Unable to read the flash ID.");
            return Err(StackError::Unexpected);
        }
        info!("The flash ID: {:x}", id);
        // Flash size is third byte in the flash ID.
        let size_info = (id >> 16) & 0xFF;
        // Check that the value is not smaller than the offset (avoids negative subtraction),
        // and ensure the result does not exceed the 32-bit shift limit.
        if size_info < FLASH_ID_SIZE_OFFSET || size_info - FLASH_ID_SIZE_OFFSET >= 32 {
            error!("Invalid flash ID.");
            return Err(StackError::Unexpected);
        }
        let flash_size = 1u32 << (size_info - FLASH_ID_SIZE_OFFSET);

        Ok(flash_size)
    }
}
