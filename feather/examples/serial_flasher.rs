//! Firmware updater for WINC1500 using serial port.

#![no_main]
#![no_std]

use bsp::shared::SpiStream;
use feather as bsp;
use feather::init::{init, UsbSerial};
use feather::{error, info, debug};

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

fn receive_packet(
    usb: &UsbSerial,
    packet: &mut SerialPacket,
    buffer: &mut [u8],
) -> Result<(), StackError> {
    let mut ctrl_buff = [0u8; SERIAL_PACKET_SIZE];

    // read the control packet
    let rcv_bytes = nb::block!(usb.read(&mut ctrl_buff))?;

    if rcv_bytes != SERIAL_PACKET_SIZE {
        return Err(StackError::WincWifiFail(CommError::ReadError));
    }

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

    debug!("Packet Received: {:?}", packet);

    // read the payload
    if packet.payload_length > 0 && packet.payload_length <= MAX_PAYLOAD_SIZE as u16 {
        let len = packet.payload_length as usize;
        nb::block!(usb.read(&mut buffer[..len]))?;
    }

    Ok(())
}

fn program() -> Result<(), StackError> {
    if let Ok(ini) = init() {
        info!("Hello, Winc Module");

        let mut stack = WincClient::new(SpiStream::new(ini.cs, ini.spi));

        let usb = UsbSerial;

        // boot the device to download mode.
        let _ = nb::block!(stack.start_in_download_mode());

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
                        nb::block!(usb.write(HELLO_CMD_REPLY))?;
                    }
                }

                SerialCommand::MaxPayloadSize => {
                    let bytes = u16::to_be_bytes(MAX_PAYLOAD_SIZE as u16);
                    nb::block!(usb.write(&bytes))?;
                }
                SerialCommand::WriteFlash => {
                    let addr = packet.address;
                    let len = packet.payload_length as usize;
                    // write to flash
                    //if stack.flash_write(addr, &buffer[..len]).is_err() {
                    if false {
                        nb::block!(usb.write(ERR_STATUS))?;
                    } else {
                        nb::block!(usb.write(OKAY_STATUS))?;
                    }
                }

                SerialCommand::ReadFlash => {
                    let addr = packet.address;
                    let len = packet.arguments as usize;
                    // clear the read buffer
                    buffer.fill(0);
                    // read the flash
                    if stack.flash_read(addr, &mut buffer[..len]).is_err() {
                        //error!("Error Occureed while reading the flash, address: {:x}, length: {}", addr, len);
                        nb::block!(usb.write(ERR_STATUS))?;
                    } else {
                        nb::block!(usb.write(&buffer[..len]))?;
                        nb::block!(usb.write(OKAY_STATUS))?;
                    }
                }

                SerialCommand::EraseFlash => {
                    // erase the flash
                    //if stack.flash_erase(packet.address, packet.arguments).is_err() {
                    if false {
                        nb::block!(usb.write(ERR_STATUS))?;
                    } else {
                        nb::block!(usb.write(OKAY_STATUS))?;
                    }
                }

                SerialCommand::Unhandled => {
                    error!("Unexpected serial command received");
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
