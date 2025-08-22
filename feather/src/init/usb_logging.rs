#[cfg(feature = "usb")]
mod usb_device_impl {
    use super::super::hal::pac::interrupt;
    use core::cell::RefCell;
    use cortex_m::interrupt::Mutex;
    use feather_m0::hal::usb::UsbBus;
    use usb_device::bus::UsbBusAllocator;
    use usb_device::device::UsbDevice;
    use usb_device::prelude::{UsbDeviceBuilder, UsbVidPid};
    use usb_device::{device::StringDescriptors, LangID};
    use usbd_serial::SerialPort;
    use usbd_serial::USB_CLASS_CDC;

    // USB globals
    static USB_BUS: Mutex<RefCell<Option<UsbDevice<UsbBus>>>> = Mutex::new(RefCell::new(None));
    pub(super) static USB_SERIAL: Mutex<RefCell<Option<SerialPort<UsbBus>>>> =
        Mutex::new(RefCell::new(None));

    pub fn setup_usb_device(
        usb_allocator: UsbBusAllocator<UsbBus>,
        nvic: &mut cortex_m::peripheral::NVIC,
    ) {
        cortex_m::interrupt::free(|cs| {
            // Use cortex_m::singleton! for safe 'static allocation
            let alloc = cortex_m::singleton!(: UsbBusAllocator<UsbBus> = usb_allocator).unwrap();

            let usb_serial = SerialPort::new(alloc);
            let usb_device = UsbDeviceBuilder::new(alloc, UsbVidPid(0x16c0, 0x27dd))
                .strings(&[StringDescriptors::new(LangID::EN_US)
                    .manufacturer("Adafruit")
                    .product("Feather M0 WiFi")
                    .serial_number("987654321")])
                .expect("Failed to set strings")
                .device_class(USB_CLASS_CDC)
                .build();

            USB_BUS.borrow(cs).replace(Some(usb_device));
            USB_SERIAL.borrow(cs).replace(Some(usb_serial));
        });

        unsafe {
            nvic.set_priority(interrupt::USB, 1);
            cortex_m::peripheral::NVIC::unmask(interrupt::USB);
        }
    }

    // USB interrupt handler
    #[interrupt]
    fn USB() {
        cortex_m::interrupt::free(|cs| {
            if let Some(bus) = USB_BUS.borrow(cs).borrow_mut().as_mut() {
                if let Some(serial) = USB_SERIAL.borrow(cs).borrow_mut().as_mut() {
                    bus.poll(&mut [serial]);
                }
            }
        });
    }
}

#[cfg(feature = "log")]
mod logging_impl {
    use super::usb_device_impl::USB_SERIAL;

    // Minimal buffer wrapper for formatting
    struct FormatBuffer {
        buffer: [u8; 128],
        pos: usize,
    }

    impl core::fmt::Write for FormatBuffer {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let remaining = self.buffer.len() - self.pos;
            let len = bytes.len().min(remaining);

            self.buffer[self.pos..self.pos + len].copy_from_slice(&bytes[..len]);
            self.pos += len;
            Ok(())
        }
    }

    struct UsbLogger {}

    impl log::Log for UsbLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= get_usb_log_level()
        }
        fn log(&self, record: &log::Record) {
            cortex_m::interrupt::free(|_cs| {
                if let Some(serial) = USB_SERIAL.borrow(_cs).borrow_mut().as_mut() {
                    let mut cursor = FormatBuffer {
                        buffer: [0u8; 128],
                        pos: 0,
                    };
                    let _ = core::fmt::write(
                        &mut cursor,
                        format_args!("[{}] {}\r\n", record.level(), record.args()),
                    );
                    let result_slice = &cursor.buffer[..cursor.pos];
                    serial.write(result_slice).ok();
                }
            });
        }
        fn flush(&self) {}
    }

    // Global logger instance
    static GLOBAL_LOGGER: UsbLogger = UsbLogger {};

    fn get_usb_log_level() -> log::Level {
        // Check environment variable at compile time
        match option_env!("FEATHER_USB_LOG") {
            Some("error") => log::Level::Error,
            Some("warn") => log::Level::Warn,
            Some("info") => log::Level::Info,
            Some("debug") => log::Level::Debug,
            Some("trace") => log::Level::Trace,
            _ => log::Level::Info, // Default to Info level
        }
    }

    fn get_usb_log_level_filter() -> log::LevelFilter {
        match get_usb_log_level() {
            log::Level::Error => log::LevelFilter::Error,
            log::Level::Warn => log::LevelFilter::Warn,
            log::Level::Info => log::LevelFilter::Info,
            log::Level::Debug => log::LevelFilter::Debug,
            log::Level::Trace => log::LevelFilter::Trace,
        }
    }

    pub fn initialize_usb_logging() {
        unsafe {
            cortex_m::interrupt::free(|_cs| {
                let _ = log::set_logger_racy(&GLOBAL_LOGGER);
                log::set_max_level_racy(get_usb_log_level_filter());
            });
        }
    }
}

#[cfg(feature = "usb-serial")]
mod usb_serial_impl {
    use super::usb_device_impl::USB_SERIAL;
    use embedded_nal::nb;
    use usb_device::UsbError;
    use wincwifi::{CommError, StackError};

    pub struct UsbSerial;

    impl UsbSerial {
        /// Read data from the USB
        ///
        /// # Arguments
        ///
        /// * `buffer` - Buffer where read data will be placed.
        ///
        /// # Returns
        ///
        /// * `usize` - Number of bytes read from the usb.
        /// * `StackError` - If any
        pub fn read(&self, buffer: &mut [u8]) -> nb::Result<usize, StackError> {
            if buffer.is_empty() {
                return Err(nb::Error::Other(StackError::InvalidParameters));
            }

            let result = cortex_m::interrupt::free(|_cs| {
                if let Some(serial) = USB_SERIAL.borrow(_cs).borrow_mut().as_mut() {
                    serial.read(buffer)
                } else {
                    // Operation is used before initializing the usb.
                    Err(UsbError::InvalidState)
                }
            });

            match result {
                Ok(size) => Ok(size),
                Err(e) => Err(match e {
                    UsbError::WouldBlock => nb::Error::WouldBlock,
                    UsbError::InvalidState => nb::Error::Other(StackError::InvalidState),
                    UsbError::BufferOverflow => {
                        nb::Error::Other(StackError::WincWifiFail(CommError::ReadError))
                    }
                    _ => nb::Error::Other(StackError::Unexpected),
                }),
            }
        }
        /// Send data to Serial Interface.
        pub fn write(&self, data: &[u8]) -> nb::Result<usize, StackError> {
            let result = cortex_m::interrupt::free(|_cs| {
                if let Some(serial) = USB_SERIAL.borrow(_cs).borrow_mut().as_mut() {
                    serial.write(data)
                } else {
                    Err(UsbError::InvalidState)
                }
            });

            match result {
                Ok(size) => Ok(size),
                Err(e) => Err(match e {
                    UsbError::WouldBlock => nb::Error::WouldBlock,
                    UsbError::InvalidState => nb::Error::Other(StackError::InvalidState),
                    UsbError::BufferOverflow => {
                        nb::Error::Other(StackError::WincWifiFail(CommError::WriteError))
                    }
                    _ => nb::Error::Other(StackError::Unexpected),
                }),
            }
        }
    }
}

// Public interface
#[cfg(feature = "usb")]
pub use usb_device_impl::setup_usb_device;

#[cfg(feature = "log")]
pub use logging_impl::initialize_usb_logging;

#[cfg(feature = "usb-serial")]
pub use usb_serial_impl::UsbSerial;
