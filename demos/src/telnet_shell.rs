use super::{debug, info};
use embedded_nal::nb::block;
use embedded_nal::TcpFullStack;

use super::SocketAddrWrap;

use menu::{self, Runner};

struct FooContext {}
struct FooOutput {}

impl embedded_io::Write for FooOutput {
    fn write(&mut self, buf: &[u8]) -> Result<usize, core::convert::Infallible> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        todo!()
    }
}
impl embedded_io::ErrorType for FooOutput {
    type Error = core::convert::Infallible;
}

const ROOT_MENU: menu::Menu<FooOutput, FooContext> = menu::Menu {
    label: "shell",
    entry: None,
    exit: None,
    items: &[],
};

pub fn telnet_shell<T, S>(stack: &mut T, port: Option<u16>) -> Result<(), T::Error>
where
    T: TcpFullStack<TcpSocket = S> + ?Sized,
{
    let mut sock = stack.socket()?;
    let port = port.unwrap_or(23);
    debug!("-----Binding to TCP port {}-----", port);
    stack.bind(&mut sock, port)?;
    info!("-----Bound to TCP port {}-----", port);

    // do listen, accept, and send/receive
    let mut line_buffer = [0; 1024];

    stack.listen(&mut sock)?;
    info!("-----Listening-----");
    loop {
        let (mut client_sock, addr) = block!(stack.accept(&mut sock))?;
        info!(
            "-----Accepted connection from {:?}-----",
            SocketAddrWrap { addr: &addr }
        );
        block!(stack.send(&mut client_sock, b"Hello to shell!\r\n"))?;

        let mut menu_buffer = [0; 128];
        let mut context = FooContext {};
        let output = FooOutput {};
        let mut runner = Runner::new(ROOT_MENU, &mut menu_buffer, output, &mut context);

        // Loop over input lines
        loop {
            let mut input_slice_index = 0;
            let mut newline_index = 0;
            match block!(stack.send(&mut client_sock, b"> ")) {
                Ok(_) => (),
                Err(e) => {
                    info!("-----Error sending prompt: {:?}-----", e);
                    break;
                }
            }

            loop {
                let recv_to_slice = &mut line_buffer[input_slice_index..];
                let received_len = match block!(stack.receive(&mut client_sock, recv_to_slice)) {
                    Ok(len) => len,
                    Err(e) => {
                        info!("-----Error receiving: {:?}-----", e);
                        break;
                    }
                };
                if received_len == 0 {
                    break;
                }
                let received_slice = &recv_to_slice[..received_len];
                let find_nl = received_slice
                    .iter()
                    .position(|&b| b == b'\n' || b == b'\r');
                if find_nl.is_some() {
                    newline_index = input_slice_index + find_nl.unwrap();
                    break;
                }
                input_slice_index += received_len;
            }
            let final_line_slice = &line_buffer[..newline_index];
            info!(
                "Received: {:?}",
                core::str::from_utf8(final_line_slice).unwrap()
            );
        }

        info!("-----Closing connection-----");
        match stack.close(client_sock) {
            Ok(_) => info!("-----Connection closed-----"),
            Err(e) => info!("-----Error closing connection: {:?}-----", e),
        }
    }
    Ok(())
}
