use embedded_nal::UdpClientStack;
use wincwifi::transfer::PrefixXfer;
use wincwifi::Socket;
#[cfg(test)]
use wincwifi::WincClient;

#[test]
fn make_socket() {
    let mut f = [0u8; 10];
    let mut writer = f.as_mut_slice();
    let mut client = WincClient::<PrefixXfer<&mut [u8]>>::new();
    let _res = client.socket();

    let x = WincClient::from_xfer(writer);
}
