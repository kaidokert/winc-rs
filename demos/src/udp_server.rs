use super::SocketAddrWrap;
use super::{debug, info};
use embedded_nal::nb::block;
use embedded_nal::UdpFullStack;

pub fn udp_server<T, S>(stack: &mut T, port: u16, loop_forever: bool) -> Result<(), T::Error>
where
    T: UdpFullStack<UdpSocket = S> + ?Sized,
{
    let mut sock = stack.socket()?;
    debug!("-----Binding to UDP port {}-----", port);
    stack.bind(&mut sock, port)?;
    info!("-----Bound to UDP port {}-----", port);

    loop {
        let mut buf = [0; 1500];
        let (n, addr) = block!(stack.receive(&mut sock, &mut buf))?;
        info!(
            "-----Received {} bytes from {:?}-----",
            n,
            SocketAddrWrap { addr: &addr }
        );

        // Extract last alphabetic character as nonce, or use 'x' as default
        let nonce = buf[..n]
            .iter()
            .rev()
            .find(|&&c| c.is_ascii_alphabetic())
            .copied()
            .unwrap_or(b'x');

        // Build response with nonce: "Hello, client_X!" where X is the nonce
        let mut response = *b"Hello, client_x!";
        response[14] = nonce; // Replace 'x' with actual nonce
        block!(stack.send_to(&mut sock, addr, &response))?;
        info!(
            "-----Sent response to {:?}-----",
            SocketAddrWrap { addr: &addr }
        );

        if !loop_forever {
            info!("Quiting the loop");
            break;
        }
        info!("Looping again");
    }

    Ok(())
}
