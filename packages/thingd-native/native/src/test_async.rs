use napi_derive::napi;
use std::thread::sleep;
use std::time::Duration;

#[napi]
pub struct TestAsync {}

#[napi]
impl TestAsync {
    #[napi]
    pub async fn do_something(&self) -> napi::Result<u32> {
        sleep(Duration::from_secs(2));
        Ok(42)
    }
}
