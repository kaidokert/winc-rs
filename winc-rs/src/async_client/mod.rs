use crate::{manager::Manager, transfer::Xfer};

mod dns;

struct AsyncClient<X: Xfer> {
    manager: Manager<X>,
}

impl<X: Xfer> AsyncClient<X> {
    pub fn new(manager: Manager<X>) -> Self {
        Self { manager }
    }
}
