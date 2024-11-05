#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Handle(pub u8);

#[derive(PartialEq, Clone, Copy)]
pub enum ClientSocketState {
    Available,
    Created,
    Connected,
}

#[derive(PartialEq, Clone, Copy)]
#[derive(defmt::Format)]
pub enum ClientSocketOp {
    None,
    New,
    Connect,
    Send,
    Recv,
    Close,
}
