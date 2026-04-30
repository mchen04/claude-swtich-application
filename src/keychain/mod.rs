#![allow(dead_code)] // keychain trait surface exercised across phases

use std::env;

use crate::error::Result;

pub const SERVICE: &str = "Claude Code-credentials";
pub const PROFILE_PREFIX: &str = "Claude Code-credentials-";

/// Claude Code stores its OAuth blob with `service="Claude Code-credentials"` and
/// `account=$USER` on macOS. Profiles saved by `cs` reuse the same service but use
/// `account="Claude Code-credentials-<profile>"` so they can be enumerated by prefix
/// without colliding with Claude Code's own entry.
pub fn canonical_account() -> String {
    env::var("USER").unwrap_or_else(|_| "claude".to_string())
}

pub fn profile_account(name: &str) -> String {
    format!("{PROFILE_PREFIX}{name}")
}

pub fn parse_profile_name(account: &str) -> Option<&str> {
    account.strip_prefix(PROFILE_PREFIX)
}

pub fn is_profile_account(account: &str) -> bool {
    account.starts_with(PROFILE_PREFIX)
}

pub trait Keychain: Send + Sync {
    fn read(&self, account: &str) -> Result<Vec<u8>>;
    fn write(&self, account: &str, secret: &[u8]) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
    fn list(&self) -> Result<Vec<String>>;
}

#[cfg(target_os = "macos")]
pub mod macos;

pub mod mock;

pub fn default_keychain() -> Box<dyn Keychain> {
    if env::var_os("CS_TEST_KEYCHAIN").is_some() {
        return Box::new(mock::MockKeychain::shared());
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacKeychain::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(mock::MockKeychain::shared())
    }
}
