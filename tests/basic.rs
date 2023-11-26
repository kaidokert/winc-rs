use embedded_nal::UdpClientStack;
use wincwifi::Socket;
#[cfg(test)]
use wincwifi::{SocketSet, WincClient};

#[test]
fn make_client() {
    let _client = WincClient::new();
}

static mut SOCKET_SET: Option<SocketSet<10>> = None;

#[test]
fn give_socket_storage() {
    let mut client = WincClient::new();

    unsafe {
        SOCKET_SET = Some(SocketSet::new());
    }

    client.set_socket_storage(unsafe { SOCKET_SET.as_mut().unwrap() });
}

#[test]
fn make_socket() {
    let mut client = WincClient::new();

    unsafe {
        SOCKET_SET = Some(SocketSet::new());
    }

    client.set_socket_storage(unsafe { SOCKET_SET.as_mut().unwrap() });

    let _res = client.socket();
}
