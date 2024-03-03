use embedded_nal::AddrType;
use embedded_nal::Dns;
use embedded_nal::IpAddr;

use crate::transfer::Xfer;
use crate::WincClient;

impl<X: Xfer> Dns for WincClient<X> {
    type Error = ();

    fn get_host_by_name(
        &mut self,
        _hostname: &str,
        _addr_type: AddrType,
    ) -> embedded_nal::nb::Result<IpAddr, Self::Error> {
        todo!();
    }

    fn get_host_by_address(
        &self,
        _addr: no_std_net::IpAddr,
        _result: &mut [u8],
    ) -> Result<usize, <Self as Dns>::Error> {
        todo!()
    }
}
