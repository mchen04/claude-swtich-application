use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::{Error, Result};

use super::Keychain;

/// In-memory mock used by unit tests within a single process.
#[derive(Default)]
pub struct MockKeychain {
    inner: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MockKeychain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> SharedMockKeychain {
        SharedMockKeychain::default()
    }
}

impl Keychain for MockKeychain {
    fn read(&self, account: &str) -> Result<Vec<u8>> {
        self.inner
            .lock()
            .unwrap()
            .get(account)
            .cloned()
            .ok_or_else(|| Error::Keychain(format!("not found: {account}")))
    }

    fn write(&self, account: &str, secret: &[u8]) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .insert(account.to_string(), secret.to_vec());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<()> {
        self.inner.lock().unwrap().remove(account);
        Ok(())
    }

    fn list(&self) -> Result<Vec<String>> {
        Ok(self.inner.lock().unwrap().keys().cloned().collect())
    }
}

/// File-backed mock keychain used by integration tests that span multiple `cs` invocations.
/// Reads/writes a JSON object `{account: stringified_blob}` at the path given by
/// `CS_TEST_KEYCHAIN_FIXTURE`. Falls back to in-memory if the env var isn't set.
#[derive(Default)]
pub struct SharedMockKeychain {
    fallback: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl SharedMockKeychain {
    fn fixture_path() -> Option<PathBuf> {
        std::env::var_os("CS_TEST_KEYCHAIN_FIXTURE").map(PathBuf::from)
    }

    fn load(&self) -> BTreeMap<String, Vec<u8>> {
        if let Some(path) = Self::fixture_path() {
            if let Ok(bytes) = fs::read(&path) {
                if let Ok(serde_json::Value::Object(map)) =
                    serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    return map
                        .into_iter()
                        .filter_map(|(k, v)| match v {
                            serde_json::Value::String(s) => Some((k, s.into_bytes())),
                            _ => None,
                        })
                        .collect();
                }
            }
            return BTreeMap::new();
        }
        self.fallback.lock().unwrap().clone()
    }

    fn save(&self, map: &BTreeMap<String, Vec<u8>>) -> Result<()> {
        if let Some(path) = Self::fixture_path() {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(
                    k.clone(),
                    serde_json::Value::String(String::from_utf8_lossy(v).into_owned()),
                );
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| Error::io_at(parent, e))?;
            }
            fs::write(&path, serde_json::to_vec(&serde_json::Value::Object(obj))?)
                .map_err(|e| Error::io_at(&path, e))?;
        } else {
            *self.fallback.lock().unwrap() = map.clone();
        }
        Ok(())
    }
}

impl Keychain for SharedMockKeychain {
    fn read(&self, account: &str) -> Result<Vec<u8>> {
        self.load()
            .get(account)
            .cloned()
            .ok_or_else(|| Error::Keychain(format!("not found: {account}")))
    }
    fn write(&self, account: &str, secret: &[u8]) -> Result<()> {
        let mut map = self.load();
        map.insert(account.to_string(), secret.to_vec());
        self.save(&map)
    }
    fn delete(&self, account: &str) -> Result<()> {
        let mut map = self.load();
        map.remove(account);
        self.save(&map)
    }
    fn list(&self) -> Result<Vec<String>> {
        Ok(self.load().into_keys().collect())
    }
}
