pub trait DelayTrait: FnMut(u32) {}
impl<U> DelayTrait for U where U: FnMut(u32) {}
