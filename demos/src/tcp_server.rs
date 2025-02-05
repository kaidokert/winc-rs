use super::{debug, info};
use embedded_nal::nb::block;
use embedded_nal::TcpFullStack;

pub fn tcp_server<T, S>(stack: &mut T, port: u16, loop_forever: bool) -> Result<(), T::Error>
where
    T: TcpFullStack<TcpSocket = S> + ?Sized,
{
    let mut sock = stack.socket()?;
    debug!("-----Binding to TCP port {}-----", port);
    stack.bind(&mut sock, port)?;
    info!("-----Bound to TCP port {}-----", port);

    // do listen, accept, and send/receive
    stack.listen(&mut sock)?;

    loop {
        let (mut client_sock, addr) = block!(stack.accept(&mut sock))?;
        info!("-----Accepted connection from {}-----", addr);

        let mut buf = [0; 1024];
        let n = block!(stack.receive(&mut client_sock, &mut buf))?;
        info!("-----Received {} bytes from {}-----", n, addr);
        let decoded = core::str::from_utf8(&buf[..n]);
        if let Ok(decoded) = decoded {
            debug!("-----Received data: {}-----", decoded);
        } else {
            debug!("-----Failed to decode-----");
        }

        let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, client!";
        block!(stack.send(&mut client_sock, response.as_bytes()))?;
        info!("-----Sent response to {}-----", addr);
        if !loop_forever {
            break;
        }
    }
    Ok(())
}
