use crate::{manager::Manager, transfer::Xfer};
use embedded_nal_async::Dns;

use super::AsyncClient;

impl<X: Xfer> Dns for AsyncClient<X> {
    type Error = u8;

    async fn get_host_by_name(
        &self,
        host: &str,
        addr_type: embedded_nal::AddrType,
    ) -> Result<core::net::IpAddr, Self::Error> {
        todo!()
    }

    async fn get_host_by_address(
        &self,
        addr: core::net::IpAddr,
        result: &mut [u8],
    ) -> Result<usize, Self::Error> {
        todo!()
    }
}
