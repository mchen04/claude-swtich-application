//! Keychain abstraction: macOS Keychain via `security-framework` plus an in-memory
//! mock for tests. All credential reads/writes go through the `Keychain` trait.

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

/// Abstraction over the macOS Keychain (or an in-memory mock for tests).
pub trait Keychain: Send + Sync {
    /// Read the secret bytes for the given account.
    fn read(&self, account: &str) -> Result<Vec<u8>>;
    /// Write secret bytes for the given account.
    fn write(&self, account: &str, secret: &[u8]) -> Result<()>;
    /// Remove the given account.
    fn delete(&self, account: &str) -> Result<()>;
    /// List all accounts under the `Claude Code-credentials` service.
    fn list(&self) -> Result<Vec<String>>;
}

/// Write `secret` to `account` and verify the round-trip matches. On mismatch the
/// entry is deleted so we don't leave a partially-written blob.
pub fn write_verified(kc: &dyn Keychain, account: &str, secret: &[u8]) -> Result<()> {
    kc.write(account, secret)?;
    match kc.read(account) {
        Ok(roundtrip) if roundtrip == secret => Ok(()),
        Ok(_) | Err(_) => {
            let _ = kc.delete(account);
            Err(crate::error::Error::Other(format!(
                "Keychain write verification failed for {account}; rolled back"
            )))
        }
    }
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
