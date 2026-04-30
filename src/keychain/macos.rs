use security_framework::os::macos::keychain::SecKeychain;
use security_framework::os::macos::passwords::find_generic_password;

use crate::error::{Error, Result};

use super::{Keychain, SERVICE};

pub struct MacKeychain;

impl MacKeychain {
    pub fn new() -> Self {
        Self
    }

    fn default_kc() -> Result<SecKeychain> {
        SecKeychain::default().map_err(|e| Error::Keychain(format!("default keychain: {e}")))
    }
}

impl Keychain for MacKeychain {
    fn read(&self, account: &str) -> Result<Vec<u8>> {
        let kc = Self::default_kc()?;
        let (pw, _) = find_generic_password(Some(&[kc]), service_for(account), account)
            .map_err(|e| Error::Keychain(format!("read {account}: {e}")))?;
        Ok(pw.as_ref().to_vec())
    }

    fn write(&self, account: &str, secret: &[u8]) -> Result<()> {
        let kc = Self::default_kc()?;
        let svc = service_for(account);
        match find_generic_password(Some(std::slice::from_ref(&kc)), svc, account) {
            Ok((_, mut item)) => item
                .set_password(secret)
                .map_err(|e| Error::Keychain(format!("update {account}: {e}"))),
            Err(_) => {
                let kc = Self::default_kc()?;
                kc.set_generic_password(svc, account, secret)
                    .map_err(|e| Error::Keychain(format!("create {account}: {e}")))
            }
        }
    }

    fn delete(&self, account: &str) -> Result<()> {
        let kc = Self::default_kc()?;
        match find_generic_password(Some(&[kc]), service_for(account), account) {
            Ok((_, item)) => {
                item.delete();
                Ok(())
            }
            Err(e) => Err(Error::Keychain(format!("delete {account}: {e}"))),
        }
    }

    fn list(&self) -> Result<Vec<String>> {
        // security-framework's high-level API does not expose enumeration of generic
        // password items by service prefix without dropping into raw CoreFoundation
        // calls. Shell out to /usr/bin/security to enumerate. This is read-only and
        // does not prompt for ACL approval.
        let output = std::process::Command::new("/usr/bin/security")
            .args(["dump-keychain"])
            .output()
            .map_err(|e| Error::Keychain(format!("dump-keychain spawn: {e}")))?;
        if !output.status.success() {
            return Err(Error::Keychain(format!(
                "dump-keychain failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut accounts = Vec::new();
        let mut current_service: Option<String> = None;
        let mut current_account: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("keychain:") {
                current_service = None;
                current_account = None;
            } else if let Some(rest) = line.strip_prefix("\"svce\"<blob>=") {
                current_service = parse_blob_value(rest);
            } else if let Some(rest) = line.strip_prefix("\"acct\"<blob>=") {
                current_account = parse_blob_value(rest);
            }
            if let (Some(svc), Some(acct)) = (&current_service, &current_account) {
                if svc == SERVICE && !accounts.contains(acct) {
                    accounts.push(acct.clone());
                }
            }
        }
        accounts.sort();
        accounts.dedup();
        Ok(accounts)
    }
}

/// Every account `cs` cares about lives under the single Claude Code service.
fn service_for(_account: &str) -> &str {
    SERVICE
}

fn parse_blob_value(rest: &str) -> Option<String> {
    let rest = rest.trim();
    if rest == "<NULL>" {
        return None;
    }
    if let Some(stripped) = rest.strip_prefix('"') {
        if let Some(end) = stripped.rfind('"') {
            return Some(stripped[..end].to_string());
        }
    }
    None
}
