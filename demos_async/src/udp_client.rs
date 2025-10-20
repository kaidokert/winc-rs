use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use embedded_nal_async::UnconnectedUdp;

#[cfg(feature = "defmt")]
use defmt::info;

#[cfg(feature = "log")]
use log::info;

/// Run async UDP client that sends data and receives response
pub async fn run_udp_client<T: UnconnectedUdp>(
    stack: &mut T,
    local_addr: SocketAddr,
    server_ip: Ipv4Addr,
    server_port: u16,
    data: &[u8],
    recv_buffer: &mut [u8],
) -> Result<usize, T::Error> {
    // Extract IP and port for defmt compatibility (SocketAddr doesn't implement Format)
    let local_ip = match local_addr {
        SocketAddr::V4(addr) => addr.ip().octets(),
        SocketAddr::V6(_) => panic!("IPv6 not supported"),
    };
    let local_port = local_addr.port();
    let server_octets = server_ip.octets();

    info!(
        "Starting UDP client: sending {} bytes from {}.{}.{}.{}:{} to {}.{}.{}.{}:{}",
        data.len(),
        local_ip[0],
        local_ip[1],
        local_ip[2],
        local_ip[3],
        local_port,
        server_octets[0],
        server_octets[1],
        server_octets[2],
        server_octets[3],
        server_port
    );

    let server_addr = SocketAddr::V4(SocketAddrV4::new(server_ip, server_port));

    // Send data to server
    stack.send(local_addr, server_addr, data).await?;

    info!(
        "Sent {} bytes to {}.{}.{}.{}:{}",
        data.len(),
        server_octets[0],
        server_octets[1],
        server_octets[2],
        server_octets[3],
        server_port
    );

    // Receive response
    info!("Waiting for response...");

    let (recv_len, local_received, remote_received) = stack.receive_into(recv_buffer).await?;

    // Extract received addresses for defmt compatibility
    let local_recv_ip = match local_received {
        SocketAddr::V4(addr) => addr.ip().octets(),
        SocketAddr::V6(_) => panic!("IPv6 not supported"),
    };
    let local_recv_port = local_received.port();

    let remote_recv_ip = match remote_received {
        SocketAddr::V4(addr) => addr.ip().octets(),
        SocketAddr::V6(_) => panic!("IPv6 not supported"),
    };
    let remote_recv_port = remote_received.port();

    info!(
        "Received {} bytes from {}.{}.{}.{}:{} to {}.{}.{}.{}:{}",
        recv_len,
        remote_recv_ip[0],
        remote_recv_ip[1],
        remote_recv_ip[2],
        remote_recv_ip[3],
        remote_recv_port,
        local_recv_ip[0],
        local_recv_ip[1],
        local_recv_ip[2],
        local_recv_ip[3],
        local_recv_port
    );

    info!("UDP client completed successfully");
    Ok(recv_len)
}
