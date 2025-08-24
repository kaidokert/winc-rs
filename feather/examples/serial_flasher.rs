//! Firmware updater for WINC1500 using serial port.
//! It is intentionally designed to work with the Arduino
//! WiFi101 Firmware Updater Java utility.

#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use feather as bsp;
use feather::init::{init, UsbSerial};
use feather::{error, info};

use wincwifi::{CommError, StackError, WincClient};

/// Size of Serial Packet (1 - Command, 4 - Address, 4 - Arguments, 2 - Payload length)
const SERIAL_PACKET_SIZE: usize = 11;
/// Maximum payload that can be received.
const MAX_PAYLOAD_SIZE: usize = 1024;
/// Address received with hello command.
const HELLO_CMD_ADDR: u32 = 0x11223344;
/// Arguments received with hello command.
const HELLO_CMD_ARG: u32 = 0x55667788;
/// Response for Hello command.
const HELLO_CMD_REPLY: &[u8] = "v10000".as_bytes();
/// Okay status sent back to script if flash operation as successfull.
const OKAY_STATUS: &[u8] = "OK".as_bytes();
/// Error status sent back to script if flash operation failed.
const ERR_STATUS: &[u8] = "ER".as_bytes();

/// Commands for communicating with flash.
#[repr(u8)]
#[derive(Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum SerialCommand {
    #[default]
    Unhandled,
    ReadFlash = 0x01,
    WriteFlash = 0x02,
    EraseFlash = 0x03,
    MaxPayloadSize = 0x50,
    Hello = 0x99,
}

/// Communication Packet
#[derive(Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
struct SerialPacket {
    command: SerialCommand,
    address: u32,
    arguments: u32,
    payload_length: u16,
}

/// Implementation to convert the u8 value to Command.
impl From<u8> for SerialCommand {
    fn from(value: u8) -> Self {
        match value {
            0x01 => Self::ReadFlash,
            0x02 => Self::WriteFlash,
            0x03 => Self::EraseFlash,
            0x50 => Self::MaxPayloadSize,
            0x99 => Self::Hello,
            _ => Self::Unhandled,
        }
    }
}

/// Implementation to convert the SerialCommand value to u8.
impl From<SerialCommand> for u8 {
    fn from(value: SerialCommand) -> Self {
        value as Self
    }
}

/// Blocking USB read until the buffer is full.
fn usb_read(usb: &UsbSerial, buffer: &mut [u8]) -> Result<(), StackError> {
    let mut len = buffer.len();
    let mut offset: usize = 0;

    while len > 0 {
        let rcvd_len = nb::block!(usb.read(&mut buffer[offset..offset + len]))?;
        len -= rcvd_len;
        offset += rcvd_len;
    }

    Ok(())
}

/// Blocking USB write until all data is written.
fn usb_write(usb: &UsbSerial, data: &[u8]) -> Result<(), StackError> {
    let mut len = data.len();
    let mut offset: usize = 0;

    while len > 0 {
        let sent_len = nb::block!(usb.write(&data[offset..offset + len]))?;
        len -= sent_len;
        offset += sent_len;
    }

    Ok(())
}

/// Receive control packet from the flasher utility.
fn receive_packet(
    usb: &UsbSerial,
    packet: &mut SerialPacket,
    buffer: &mut [u8],
) -> Result<(), StackError> {
    let mut ctrl_buff = [0u8; SERIAL_PACKET_SIZE];

    // read the control packet
    usb_read(&usb, &mut ctrl_buff)?;

    // Extract parameters of control packet.
    packet.command = ctrl_buff[0].into();
    packet.address = u32::from_be_bytes(
        ctrl_buff[1..5]
            .try_into()
            .map_err(|_| StackError::Unexpected)?,
    );
    packet.arguments = u32::from_be_bytes(
        ctrl_buff[5..9]
            .try_into()
            .map_err(|_| StackError::Unexpected)?,
    );
    packet.payload_length = u16::from_be_bytes(
        ctrl_buff[9..]
            .try_into()
            .map_err(|_| StackError::Unexpected)?,
    );

    // read the payload
    if packet.payload_length > 0 {
        let len = packet.payload_length as usize;
        if len > buffer.len() {
            return Err(StackError::WincWifiFail(CommError::BufferError));
        }
        usb_read(&usb, &mut buffer[..len])?;
    }

    Ok(())
}

fn program() -> Result<(), StackError> {
    if let Ok(ini) = init() {
        info!("Hello, Winc Module");

        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi));

        let usb = UsbSerial;

        // boot the device to download mode.
        nb::block!(stack.start_in_download_mode())?;

        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let mut packet = SerialPacket::default();

        loop {
            // clear the read buffer
            buffer.fill(0);
            // receive the packet
            receive_packet(&usb, &mut packet, &mut buffer)?;

            match packet.command {
                SerialCommand::Hello => {
                    if packet.address == HELLO_CMD_ADDR && packet.arguments == HELLO_CMD_ARG {
                        usb_write(&usb, HELLO_CMD_REPLY)?;
                    }
                }

                SerialCommand::MaxPayloadSize => {
                    let bytes = u16::to_be_bytes(MAX_PAYLOAD_SIZE as u16);
                    usb_write(&usb, &bytes)?;
                }

                SerialCommand::WriteFlash => {
                    let addr = packet.address;
                    let len = packet.payload_length as usize;
                    // write to flash
                    if stack.flash_write(addr, &buffer[..len]).is_err() {
                        error!(
                            "Error occurred while writing to the flash. Address: {:x}, length: {}",
                            addr, len
                        );
                        usb_write(&usb, ERR_STATUS)?;
                    } else {
                        usb_write(&usb, OKAY_STATUS)?;
                    }
                }

                SerialCommand::ReadFlash => {
                    let addr = packet.address;
                    let len = packet.arguments as usize;

                    if len > buffer.len() {
                        error!("Invalid data length received.");
                        return Err(StackError::WincWifiFail(CommError::BufferError));
                    }

                    // clear the read buffer
                    buffer.fill(0);
                    // read the flash
                    if stack.flash_read(addr, &mut buffer[..len]).is_err() {
                        error!(
                            "Error occurred while reading the flash. Address: {:x}, length: {}",
                            addr, len
                        );
                        usb_write(&usb, ERR_STATUS)?;
                    } else {
                        // write the read bytes
                        usb_write(&usb, &buffer[..len])?;
                        // send okay status
                        usb_write(&usb, OKAY_STATUS)?;
                    }
                }

                SerialCommand::EraseFlash => {
                    // erase the flash
                    if stack.flash_erase(packet.address, packet.arguments).is_err() {
                        error!(
                            "Error occurred while erasing the flash. Address: {:x}, length: {}",
                            packet.address, packet.arguments
                        );
                        usb_write(&usb, ERR_STATUS)?;
                    } else {
                        usb_write(&usb, OKAY_STATUS)?;
                    }
                }

                SerialCommand::Unhandled => {
                    error!("Unknown serial command received.");
                    return Err(StackError::Unexpected);
                }
            }
        }
    }
    Ok(())
}

#[cortex_m_rt::entry]
fn main() -> ! {
    if let Err(err) = program() {
        error!("Error: {}", err);
        panic!("Error in main program");
    } else {
        info!("Good exit")
    };
    loop {}
}
