pub mod installer;
pub mod sanitizer;
pub mod spool;
pub mod verifier;

pub type BridgeResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn invalid(message: &str) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}
/// Passive session lifecycle and exec-form hook transport verified on Windows.
/// Prompt/tool/attention outcomes remain outside the live Agent acceptance gate.
pub const DOCUMENTED_FIXTURE_VERSION: &str = "2.1.287";
pub const WAGERING_AUTHORITY: &str = "native-validated-session-and-evidence-policy";
