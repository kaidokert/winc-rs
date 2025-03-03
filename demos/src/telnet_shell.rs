use super::{debug, info};
use embedded_nal::nb;
use embedded_nal::nb::block;
use embedded_nal::TcpFullStack;
use menu::Item;
use menu::ItemType;
use menu::Menu;

use super::SocketAddrWrap;

use menu::{self, Runner};

pub struct Context {
    close: bool,
}

// Owns a connected client socket
pub struct Output<'a, T: TcpFullStack + Sized> {
    stack: &'a mut T,
    sock: T::TcpSocket,
}

impl<'a, T: TcpFullStack + Sized> Output<'a, T> {
    fn send(&mut self, buf: &[u8]) -> Result<usize, nb::Error<T::Error>> {
        self.stack.send(&mut self.sock, buf)
    }
    fn recv(&mut self, buf: &mut [u8]) -> Result<usize, nb::Error<T::Error>> {
        self.stack.receive(&mut self.sock, buf)
    }
    fn close(self) -> Result<(), T::Error> {
        self.stack.close(self.sock)?;
        Ok(())
    }
}

mod errwrap;
use errwrap::ErrWrap;
impl<'a, T: TcpFullStack + Sized> embedded_io::ErrorType for Output<'a, T>
where
    T: TcpFullStack + Sized,
{
    type Error = ErrWrap<T, T::Error>;
}

impl<'a, T: TcpFullStack + Sized> embedded_io::Write for Output<'a, T>
where
    T: TcpFullStack + Sized,
    T::Error: core::fmt::Debug,
{
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for byte in buf {
            match byte {
                // translate \n to \r\n
                b'\n' => {
                    nb::block!(self.send(b"\r\n"))?;
                }
                _ => {
                    nb::block!(self.send(&[*byte]))?;
                }
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn enter_root<T: TcpFullStack + Sized>(
    _menu: &Menu<Output<T>, Context>,
    interface: &mut Output<T>,
    _context: &mut Context,
) {
    let _ = interface.send(b"Hello to shell! >\r\n");
}

pub fn telnet_shell<T, S>(stack: &mut T, port: Option<u16>) -> Result<(), T::Error>
where
    T: TcpFullStack<TcpSocket = S> + Sized,
{
    let mut sock = stack.socket()?;
    let port = port.unwrap_or(23);
    debug!("-----Binding to TCP port {}-----", port);
    stack.bind(&mut sock, port)?;
    info!("-----Bound to TCP port {}-----", port);

    let mut recv_buffer = [0; 1024];

    stack.listen(&mut sock)?;
    info!("-----Listening-----");
    loop {
        let root_menu: menu::Menu<Output<T>, Context> = menu::Menu {
            label: "shell",
            entry: Some(enter_root),
            exit: None, // Root menu does not exit
            items: &[
                &Item {
                    command: "foo",
                    help: Some("My foo"),
                    item_type: ItemType::Callback {
                        parameters: &[],
                        function: |_menu, _item, _args, interface, _context| {
                            let _ = interface.send(b"Foo response\r\n");
                            info!("Foo");
                        },
                    },
                },
                &Item {
                    command: "close",
                    help: Some("Close the connection"),
                    item_type: ItemType::Callback {
                        parameters: &[],
                        function: |_menu, _item, _args, _interface, context| {
                            info!("Closing connection");
                            context.close = true;
                        },
                    },
                },
            ],
        };

        let (client_sock, addr) = block!(stack.accept(&mut sock))?;
        info!(
            "-----Accepted connection from {:?}-----",
            SocketAddrWrap { addr: &addr }
        );
        let mut menu_buffer = [0; 128];
        let mut context = Context { close: false };
        let output = Output {
            stack: stack,
            sock: client_sock,
        };
        let mut runner = Runner::new(root_menu, &mut menu_buffer, output, &mut context);
        loop {
            let received_len = match block!(runner.interface.recv(recv_buffer.as_mut_slice())) {
                Ok(len) => len,
                Err(e) => {
                    info!("-----Error receiving: -----");
                    return Err(e);
                }
            };
            if received_len == 0 {
                break;
            }
            let received_slice = &recv_buffer[..received_len];
            for char in received_slice {
                match char {
                    b'\n' => { // Ignore \n
                    }
                    b'\x00' | b'\x1b' => {
                        break;
                    } // ctrl chars
                    _ => {
                        runner.input_byte(*char, &mut context);
                    }
                }
            }
            if context.close {
                break;
            }
        }

        info!("-----Closing connection-----");
        match runner.interface.close() {
            Ok(_) => info!("-----Connection closed-----"),
            Err(e) => {
                info!("-----Error closing connection: -----");
                return Err(e);
            }
        };
    }
    // loop forever above
    #[allow(unreachable_code)]
    Ok(())
}
