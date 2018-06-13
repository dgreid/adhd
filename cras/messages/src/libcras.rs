pub enum Error {
    ServerConnectFailed,
}
pub type Result<T> = std::result::Result<T, Error>;

pub struct CrasClient {
}

impl CrasClient {
    pub fn new() -> Result<Self> {
    }
}

impl Drop for CrasClient {
    pub fn drop(self) {
        // disconnect
    }
}
