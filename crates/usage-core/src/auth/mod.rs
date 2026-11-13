//! Credential storage abstraction.
//!
//! **Module 01 stores no credentials.** No selected provider needs one (see SECURITY.md).
//! This foundation exists, tested with fake secrets, for a future provider that uses a legitimate
//! credential such as an Anthropic Admin API key. We NEVER store Claude.ai passwords, cookies or
//! OAuth tokens (docs/DATA_SOURCE_RESEARCH.md §4).

#[cfg(feature = "keyring-store")]
pub mod keyring_store;

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use crate::errors::{Result, UsageError};

/// A secret string. `Debug`/`Display` are redacted so it cannot leak through logging or `{:?}`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// The only way to read the value. Call sites are the audit surface.
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Identifies one stored credential: e.g. provider `anthropic-admin-api`, account `admin-key`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CredentialKey {
    pub provider_id: String,
    pub name: String,
}

impl CredentialKey {
    pub fn new(provider_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            name: name.into(),
        }
    }
}

pub trait CredentialStore: Send + Sync {
    fn set(&self, key: &CredentialKey, secret: &Secret) -> Result<()>;
    fn get(&self, key: &CredentialKey) -> Result<Option<Secret>>;
    /// Deleting a missing credential is not an error.
    fn delete(&self, key: &CredentialKey) -> Result<()>;
}

/// In-memory store for tests and for platforms without a keyring. Never persisted.
#[derive(Default)]
pub struct InMemoryCredentialStore(Mutex<HashMap<CredentialKey, Secret>>);

impl CredentialStore for InMemoryCredentialStore {
    fn set(&self, key: &CredentialKey, secret: &Secret) -> Result<()> {
        self.0
            .lock()
            .map_err(poisoned)?
            .insert(key.clone(), secret.clone());
        Ok(())
    }
    fn get(&self, key: &CredentialKey) -> Result<Option<Secret>> {
        Ok(self.0.lock().map_err(poisoned)?.get(key).cloned())
    }
    fn delete(&self, key: &CredentialKey) -> Result<()> {
        self.0.lock().map_err(poisoned)?.remove(key);
        Ok(())
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> UsageError {
    UsageError::CredentialStorage("credential store lock poisoned".into())
}

/// Shared conformance suite: every `CredentialStore` implementation must pass this.
pub fn conformance_suite(store: &dyn CredentialStore, key: &CredentialKey) {
    let s = Secret::new("test-secret-value-123");
    store.delete(key).unwrap(); // deleting a missing entry is fine
    assert_eq!(store.get(key).unwrap(), None);
    store.set(key, &s).unwrap();
    assert_eq!(store.get(key).unwrap().unwrap().expose(), "test-secret-value-123");
    store.set(key, &Secret::new("replaced")).unwrap();
    assert_eq!(store.get(key).unwrap().unwrap().expose(), "replaced");
    store.delete(key).unwrap();
    assert_eq!(store.get(key).unwrap(), None);
    store.delete(key).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_never_appears_in_debug_or_display() {
        let s = Secret::new("sk-ant-admin01-super-secret");
        assert!(!format!("{s:?}").contains("super-secret"));
        assert!(!format!("{s}").contains("super-secret"));
        let k = CredentialKey::new("p", "n");
        let wrapped = format!("{:?}", (k, s));
        assert!(!wrapped.contains("super-secret"));
    }

    #[test]
    fn in_memory_store_conforms() {
        conformance_suite(
            &InMemoryCredentialStore::default(),
            &CredentialKey::new("test-provider", "test-key"),
        );
    }
}
