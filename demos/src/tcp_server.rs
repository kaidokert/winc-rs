use super::{debug, info};
use embedded_nal::nb::block;
use embedded_nal::TcpFullStack;

use super::SocketAddrWrap;

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
    info!("-----Listening-----");

    loop {
        let (mut client_sock, addr) = block!(stack.accept(&mut sock))?;
        info!(
            "-----Accepted connection from {:?}-----",
            SocketAddrWrap { addr: &addr }
        );

        let mut buf = [0; 1024];
        let n = block!(stack.receive(&mut client_sock, &mut buf))?;
        info!(
            "-----Received {} bytes from {:?}-----",
            n,
            SocketAddrWrap { addr: &addr }
        );
        let decoded = core::str::from_utf8(&buf[..n]);
        if let Ok(decoded) = decoded {
            debug!("-----Received data: {}-----", decoded);
        } else {
            debug!("-----Failed to decode-----");
        }

        // Extract last alphabetic character as nonce, or use 'x' as default
        let nonce = buf[..n]
            .iter()
            .rev()
            .find(|&&c| c.is_ascii_alphabetic())
            .copied()
            .unwrap_or(b'x');

        // Build response body with nonce: "Hello, client_X!" where X is the nonce
        let mut body = [0u8; 16];
        body[..14].copy_from_slice(b"Hello, client_");
        body[14] = nonce;
        body[15] = b'!';

        // Build full HTTP response
        let header = b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\n\r\n";
        let header_len = header.len(); // 39 bytes
        let mut response = [0u8; 55]; // header (39) + body (16)
        response[..header_len].copy_from_slice(header);
        response[header_len..].copy_from_slice(&body);

        block!(stack.send(&mut client_sock, &response))?;
        info!(
            "-----Sent response to {:?}-----",
            SocketAddrWrap { addr: &addr }
        );
        stack.close(client_sock)?;
        if !loop_forever {
            info!("Quiting the loop");
            break;
        }
        info!("Looping again");
    }
    Ok(())
}
