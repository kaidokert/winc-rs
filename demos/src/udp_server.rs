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

        let response = "Hello, client!";
        block!(stack.send_to(&mut sock, addr, response.as_bytes()))?;
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
