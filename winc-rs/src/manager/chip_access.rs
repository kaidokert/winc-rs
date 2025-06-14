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

use crate::errors::Error;

use crate::transfer::*;

use crc_any::CRC;

use crate::HexWrap;
use crate::{trace, warn};

fn find_first_neq_index<T: PartialEq>(a1: &[T], a2: &[T]) -> Option<usize> {
    a1.iter().zip(a2.iter()).position(|(a, b)| a != b)
}

fn crc7(input: &[u8]) -> u8 {
    let mut crc = CRC::crc7();
    crc.digest(&[0x43u8]); // reset crc to 0x7F
    crc.digest(input);
    (crc.get_crc() << 1) as u8
}

fn crc16(input: &[u8]) -> u16 {
    let mut crc = CRC::crc16aug_ccitt();
    crc.digest(&[0x99, 0xc0]); // reset crc to 0xFFFF
    crc.digest(input);
    crc.get_crc() as u16
}

#[derive(Copy, Clone)]
pub enum Cmd {
    RegRead = 0xca,
    RegWrite = 0xc9,
    DmaWrite = 0xc7,
    DmaRead = 0xc8,
}

pub struct ChipAccess<X: Xfer> {
    xfer: X,
    pub crc: bool,
    pub check_crc: bool,
    pub verify: bool,
}

impl<X: Xfer> ChipAccess<X> {
    pub fn new(xfer: X) -> Self {
        Self {
            xfer,
            crc: true,
            check_crc: false,
            verify: true,
        }
    }
    #[cfg(test)]
    pub fn set_unit_test_mode(&mut self) {
        self.crc = false;
        self.verify = false;
        self.check_crc = false;
    }
    // Todo: remove this
    pub fn delay_us(&mut self, delay: u32) {
        self.xfer.delay_us(delay);
    }

    fn protocol_verify(
        &mut self,
        msg: &'static str,
        buffer: &[u8],
        expected: &[u8],
    ) -> Result<(), Error> {
        if !self.verify {
            return Ok(());
        }
        if let Some(i) = find_first_neq_index(buffer, expected) {
            warn!("protocol_verify failed {}", msg);
            Err(Error::ProtocolByteError(msg, i, expected[i], buffer[i]))
        } else {
            Ok(())
        }
    }

    #[cfg(feature = "irq")]
    /// Wait for Interrupt on IRQ Pin
    pub fn wait_for_interrupt(&mut self) {
        #[cfg(not(test))]
        self.xfer.wait_for_interrupt()
    }

    pub fn switch_to_high_speed(&mut self) {
        self.xfer.switch_to_high_speed();
    }
    // todo: change reg arg to enum
    pub fn single_reg_read(&mut self, reg: u32) -> Result<u32, Error> {
        let r = reg.to_le_bytes();
        let mut cmd = [Cmd::RegRead as u8, r[2], r[1], r[0], 0];
        if self.crc {
            cmd[4] = crc7(&cmd[..4]);
            self.xfer.send(&cmd)?;
        } else {
            self.xfer.send(&cmd[..4])?;
        }

        let mut rdbuf = [0xFF; 1];
        self.xfer.recv(&mut rdbuf)?;
        trace!("Cmd Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("single_reg_read:cmd", &rdbuf, &[Cmd::RegRead as u8])?;

        self.xfer.recv(&mut rdbuf)?;
        trace!("Status Zero Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("single_reg_read:zero", &rdbuf, &[0])?;

        self.xfer.recv(&mut rdbuf)?;
        trace!("Data ack Bytes: {:x}", HexWrap { v: &rdbuf });
        // todo: chould check for low bits
        rdbuf[0] &= 0xF0; // DATA_START_CTRL reg value. Lower bits are start/mid/end indicator
        self.protocol_verify("single_reg_read:ack", &rdbuf, &[0xF0])?;

        let mut data_buf = [0x00; 4];
        self.xfer.recv(&mut data_buf)?;

        if self.crc {
            let mut crcbuf = [0xFF; 2];
            self.xfer.recv(&mut crcbuf)?;
            trace!("Crc Bytes: {:x}", HexWrap { v: &crcbuf });
            if self.check_crc {
                let calculated = crc16(&data_buf);
                self.protocol_verify(
                    "single_reg_read:crc16 check",
                    &crcbuf,
                    &calculated.to_be_bytes(),
                )?;
            }
        }
        Ok(u32::from_le_bytes(data_buf))
    }

    // todo: change register argument to enum
    pub fn single_reg_write(&mut self, reg: u32, val: u32) -> Result<(), Error> {
        // info!("write {:x} val {:x}", reg, val);
        let v = val.to_le_bytes();
        let r = reg.to_le_bytes();
        let mut cmd = [
            Cmd::RegWrite as u8,
            r[2],
            r[1],
            r[0],
            v[3],
            v[2],
            v[1],
            v[0],
            0x00,
        ];
        if self.crc {
            cmd[8] = crc7(&cmd[..8]);
            self.xfer.send(&cmd)?;
        } else {
            self.xfer.send(&cmd[..8])?;
        }

        let mut rdbuf = [0x0; 1];
        self.xfer.recv(&mut rdbuf)?;
        trace!("Cmd Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("single_reg_write:cmd echo", &rdbuf, &[Cmd::RegWrite as u8])?;

        rdbuf[0] = 0;
        self.xfer.recv(&mut rdbuf)?;
        trace!("Status zero Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("single_reg_write:zero echo", &rdbuf, &[0])?;
        // note : response doesn't have ACK or CRC
        Ok(())
    }
    pub fn dma_block_read(&mut self, reg: u32, data: &mut [u8]) -> Result<(), Error> {
        let r = reg.to_le_bytes();
        let v = (data.len() as u32).to_le_bytes();
        let mut cmd = [Cmd::DmaRead as u8, r[2], r[1], r[0], v[2], v[1], v[0], 00];
        if self.crc {
            cmd[7] = crc7(&cmd[..7]);
            self.xfer.send(&cmd)?;
        } else {
            self.xfer.send(&cmd[..7])?;
        }

        let mut rdbuf = [0x0; 1];
        self.xfer.recv(&mut rdbuf)?;
        trace!("Cmd Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("dma_block_read:cmd", &rdbuf, &[Cmd::DmaRead as u8])?;

        rdbuf[0] = 0;
        self.xfer.recv(&mut rdbuf)?;
        trace!("Zero Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("dma_block_read:zero", &rdbuf, &[0])?;

        rdbuf[0] = 0;
        self.xfer.recv(&mut rdbuf)?;
        trace!("Ack Bytes: {:x}", HexWrap { v: &rdbuf });
        rdbuf[0] &= 0xF0; // DATA_START_CTRL reg value. Lower bits are start=1/mid=2/end=3, usually 3
        self.protocol_verify("dma_block_read:ack", &rdbuf, &[0xF0])?;

        self.xfer.recv(data)?;
        trace!("Data Bytes: {:x}", HexWrap { v: data });

        if self.crc {
            let mut crcbuf = [0x0; 2];
            self.xfer.recv(&mut crcbuf)?;
            trace!("Crc Bytes: {:x}", HexWrap { v: &crcbuf });
            if self.check_crc {
                let calculated = crc16(data);
                self.protocol_verify(
                    "dma_block_read::crc16 check",
                    &crcbuf,
                    &calculated.to_be_bytes(),
                )?;
            }
        }
        Ok(())
    }

    pub fn dma_block_write(&mut self, reg: u32, data: &[u8]) -> Result<(), Error> {
        let r = reg.to_le_bytes();
        let v = (data.len() as u32).to_le_bytes();
        let mut cmd = [Cmd::DmaWrite as u8, r[2], r[1], r[0], v[2], v[1], v[0], 0];
        if self.crc {
            cmd[7] = crc7(&cmd[..7]);
            self.xfer.send(&cmd)?;
        } else {
            self.xfer.send(&cmd[..7])?;
        }
        let mut rdbuf = [0x0; 1];
        self.xfer.recv(&mut rdbuf)?;
        trace!("Cmd Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("dma_block_write:cmd", &rdbuf, &[Cmd::DmaWrite as u8])?;

        rdbuf[0] = 0;
        self.xfer.recv(&mut rdbuf)?;
        trace!("Status Zero Bytes: {:x}", HexWrap { v: &rdbuf });
        self.protocol_verify("dma_block_write:zero", &rdbuf, &[0])?;

        trace!("Sending F3 marker");
        self.xfer.send(&[0xf3])?; // todo: could be 1/2/3, depending on conditions

        trace!("Sending data ...");
        self.xfer.send(data)?;

        let mut ack_array = [0x00; 3];
        let mut dmaackbuf = &mut ack_array[..];
        let mut expected_ack = [0x00, 0xC3, 00].as_slice();
        if self.crc {
            let calc = crc16(data).to_be_bytes();
            let dummy_crc = [calc[0], calc[1]];
            self.xfer.send(&dummy_crc)?;
            dmaackbuf = &mut ack_array[..2];
            expected_ack = &[0xC3, 00];
        }
        trace!("Getting {} ack bytes", dmaackbuf.len());
        self.xfer.recv(dmaackbuf)?;
        trace!("Dma ack Bytes: {:x}", HexWrap { v: dmaackbuf });
        self.protocol_verify("dma_block_write:ack", dmaackbuf, expected_ack)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use arrayvec::{ArrayVec, CapacityError};

    #[cfg(feature = "std")]
    use std::{thread, time};

    use super::*;

    type TmpBuffer = ArrayVec<u8, 256>;

    fn concat<'a>(
        dest: &'a mut TmpBuffer,
        slice1: &[u8],
        slice2: &[u8],
    ) -> Result<&'a TmpBuffer, CapacityError> {
        dest.clear();
        dest.try_extend_from_slice(slice1)?;
        dest.try_extend_from_slice(slice2)?;
        Ok(dest)
    }

    /// Debug implementation of Xfer trait.
    ///
    /// Prefixes read/write with a 3-byte header ( good for sending
    /// over the wire to a development host or such )
    pub(crate) struct PrefixXfer<T: ReadWrite> {
        stream: T,
    }
    impl<T: ReadWrite> PrefixXfer<T> {
        pub fn new(stream: T) -> Self {
            PrefixXfer { stream }
        }
    }

    impl<T: ReadWrite> Xfer for PrefixXfer<T> {
        fn recv(&mut self, dest: &mut [u8]) -> Result<(), Error> {
            let rd_cmnd = [0xA2, 0x00, dest.len() as u8];
            self.stream.write(&rd_cmnd).map_err(|_| Error::WriteError)?;
            self.stream.read_exact(dest).map_err(|_| Error::ReadError)?;
            Ok(())
        }

        fn send(&mut self, src: &[u8]) -> Result<(), Error> {
            let wr_cmnd = [0x81, 00, src.len() as u8];
            let wr_slice = &wr_cmnd[..];
            let mut buf = TmpBuffer::new();
            concat(&mut buf, wr_slice, src)?;
            self.stream
                .write(buf.as_slice())
                .map_err(|_| Error::WriteError)?;

            #[cfg(feature = "std")]
            thread::sleep(time::Duration::from_millis(10));

            Ok(())
        }
    }

    #[test]
    fn test_concat() {
        let mut array = TmpBuffer::new();

        assert_eq!(
            concat(&mut array, &[1u8; 2], &[2u8; 3]).unwrap().as_slice(),
            &[1, 1, 2, 2, 2]
        );
    }

    fn mkreg(reg: u32) -> [u8; 5] {
        let mut cmd = [0u8; 5];
        let regu32 = (reg as u32).to_le_bytes();
        cmd[0] = 0xCA;
        cmd[1] = regu32[2];
        cmd[2] = regu32[1];
        cmd[3] = regu32[0];
        cmd[4] = crc7(&cmd[0..4]);
        cmd
    }
    #[test]
    fn test_crc() {
        let f = mkreg(0x1000);
        assert_eq!(f[4], 0xCA);
        let f = mkreg(0x13f4);
        assert_eq!(f[4], 0xA4);
        let f = mkreg(0xe824);
        assert_eq!(f[4], 0xBC);
    }

    #[test]
    fn test_crc16() {
        let regarr = [
            0xca, 0x0, 0xf3, /*data */ 0xa0, 0x3, 0x10, 0x0, /*crc*/ 0x34, 0x95,
        ];
        let regvalue = &regarr[3..7];
        let crcvalue = [0x34, 0x95];

        let calc = crc16(regvalue);

        let readval = u16::from_be_bytes(crcvalue);
        assert_eq!(readval, calc);
    }

    #[test]
    fn test_read_chip() {
        let mut writebuf = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xCA, 0xFF, 0xFF, 0xFF,
            0x00, 0xFF, 0xFF, 0xFF, 0xF0, 0xFF, 0xFF, 0xFF, 0x01, 0x02, 0x03, 0x04,
        ];
        let writer = writebuf.as_mut_slice();
        let mut chip = ChipAccess::new(PrefixXfer::new(writer));
        chip.crc = false;
        let res = chip.single_reg_read(0x10).unwrap();
        assert_eq!(res, 0x04030201);
        assert_eq!(
            writebuf[..],
            [
                0x81, 0, 4, 0xCA, 0, 0, 0x10, 0xA2, 0, 1, 0xCA, 0xA2, 0, 1, 0, 0xA2, 0, 1, 0xF0,
                0xA2, 0, 4, 1, 2, 3, 4
            ]
        );
    }

    #[test]
    fn test_read_chip_crc() {
        let mut writebuf = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xca, 0xFF, 0xFF,
            0xFF, 0x00, 0xFF, 0xFF, 0xFF, 0xF0, 0xFF, 0xFF, 0xFF, 0x01, 0x02, 0x03, 0x04, 0xFF,
            0xFF, 0xFF, 68, 0x1,
        ];
        let writer = writebuf.as_mut_slice();

        let mut chip = ChipAccess::new(PrefixXfer::new(writer));
        let res = chip.single_reg_read(0x10).unwrap();
        assert_eq!(res, 0x04030201);
        assert_eq!(
            writebuf[..],
            [
                0x81, 0, 5, 0xCA, 0, 0, 0x10, 138, 0xA2, 0, 1, 0xCA, 0xA2, 0, 1, 0x00, 0xA2, 0, 1,
                0xF0, 0xA2, 0, 4, 1, 2, 3, 4, 0xA2, 0, 2, 68, 1
            ]
        );
    }

    #[test]
    fn test_read_chip_simple() {
        let mut writebuf = [0xFF, 0xFF, 0xFF, 0xFF, 0xCA, 0x0, 0xF0, 1, 2, 3, 4];
        let writer = writebuf.as_mut_slice();
        let mut chip = ChipAccess::new(writer);
        chip.crc = false;
        let res = chip.single_reg_read(0x10);
        assert_eq!(res, Ok(0x04030201));
    }
    #[test]
    fn test_read_chip_simple_crc() {
        let mut writebuf = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, /*echo cmd byte */ 0xCA, /*status */ 0x0,
            /*data status*/ 0xF3, /*data */ 1, 2, 3, 4, /*2 byte crc*/ 42, 0,
        ];
        let writer = writebuf.as_mut_slice();
        let mut chip = ChipAccess::new(writer);
        let res = chip.single_reg_read(0x10);
        assert_eq!(res, Ok(0x04030201));
    }
}
