use super::AsyncClient;
use super::StackError;
use crate::manager::{BootMode, BootState};
use crate::stack::socket_callbacks::WifiModuleState;
use crate::transfer::Xfer;

// todo: deduplicate this
// 1 minute max, if no other delays are added
const AP_CONNECT_TIMEOUT_MILLISECONDS: u32 = 60_000;

impl<X: Xfer> AsyncClient<'_, X> {
    pub async fn start_wifi_module(&mut self) -> Result<(), StackError> {
        if self.callbacks.borrow().state != WifiModuleState::Reset {
            return Err(StackError::InvalidState);
        }
        self.callbacks.borrow_mut().state = WifiModuleState::Starting;
        self.manager.borrow_mut().set_crc_state(true);

        let mut state = BootState::new(BootMode::Normal);
        loop {
            let result = self.manager.borrow_mut().boot_the_chip(&mut state)?;
            if result {
                self.callbacks.borrow_mut().state = WifiModuleState::Unconnected;
                return Ok(());
            }
            self.dispatch_events()?;
            self.yield_once().await; // todo: busy loop, maybe should delay here
        }
    }
    pub async fn connect_to_saved_ap(&mut self) -> Result<(), StackError> {
        if matches!(
            self.callbacks.borrow().state,
            WifiModuleState::Reset | WifiModuleState::Starting
        ) {
            return Err(StackError::InvalidState);
        }
        let mut countdown = AP_CONNECT_TIMEOUT_MILLISECONDS;
        self.callbacks.borrow_mut().state = WifiModuleState::ConnectingToAp;
        self.manager.borrow_mut().send_default_connect()?;
        loop {
            countdown -= 1;
            if countdown == 0 {
                return Err(StackError::GeneralTimeout);
            }
            let read_state = self.callbacks.borrow().state.clone();
            match read_state {
                WifiModuleState::ConnectionFailed => {
                    let mut callbacks = self.callbacks.borrow_mut();
                    // conn_error should always be Some in ConnectionFailed state,
                    // but use defensive fallback just in case
                    let res = callbacks
                        .connection_state
                        .conn_error
                        .take()
                        .unwrap_or(crate::manager::WifiConnError::Unhandled);
                    return Err(StackError::ApJoinFailed(res));
                }
                WifiModuleState::ConnectedToAp => {
                    return Ok(());
                }
                _ => {}
            }
            self.dispatch_events()?;
            self.yield_once().await;
        }
    }
}
