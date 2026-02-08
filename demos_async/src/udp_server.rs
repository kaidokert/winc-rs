use embedded_nal_async::UnconnectedUdp;

#[cfg(feature = "defmt")]
use defmt::info;

#[cfg(feature = "log")]
use log::info;

/// Run async UDP server that receives packets and responds with custom message
pub async fn run_udp_server<T: UnconnectedUdp>(
    stack: &mut T,
    port: u16,
    loop_forever: bool,
    recv_buffer: &mut [u8],
) -> Result<(), T::Error> {
    info!("-----Listening on UDP port {}-----", port);

    loop {
        // Receive packet (returns len, local addr, remote addr)
        let (n, local, remote) = stack.receive_into(recv_buffer).await?;

        // Extract remote port for logging
        let remote_port = remote.port();
        info!("-----Received {} bytes from port {}-----", n, remote_port);

        // Extract last alphabetic character as nonce, or use 'x' as default
        let nonce = recv_buffer[..n]
            .iter()
            .rev()
            .find(|&&c| c.is_ascii_alphabetic())
            .copied()
            .unwrap_or(b'x');

        // Build response with nonce: "Hello, client_X!" where X is the nonce
        let mut response = *b"Hello, client_x!";
        response[14] = nonce; // Replace 'x' with actual nonce

        // Send response back to sender (local, remote, data)
        stack.send(local, remote, &response).await?;
        info!("-----Sent response to port {}-----", remote_port);

        if !loop_forever {
            info!("Quitting the loop");
            break;
        }
        info!("Looping again");
    }

    Ok(())
}
