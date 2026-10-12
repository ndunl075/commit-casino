pub mod installer;
pub mod sanitizer;
pub mod spool;
pub mod verifier;

pub type BridgeResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn invalid(message: &str) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}
pub const DOCUMENTED_FIXTURE_VERSION: &str = "2.1.295";
pub const LIVE_AGENT_WAGERING: bool = false;
