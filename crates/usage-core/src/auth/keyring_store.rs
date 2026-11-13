//! OS credential storage via the `keyring` crate:
//! Windows Credential Manager, macOS Keychain, Linux Secret Service.
//!
//! Platform behaviour must be validated on each OS (the CI `ignored` tests do this); passing on
//! Linux says nothing about Windows Credential Manager.

use keyring::Entry;

use super::*;

pub struct KeyringCredentialStore {
    service: String,
}

impl KeyringCredentialStore {
    /// `service` namespaces our entries in the OS store (e.g. `com.usagemonitormaximus.app`).
    pub fn new(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }

    fn entry(&self, key: &CredentialKey) -> Result<Entry> {
        Entry::new(&self.service, &format!("{}/{}", key.provider_id, key.name)).map_err(map_err)
    }
}

fn map_err(e: keyring::Error) -> UsageError {
    // keyring error text never contains the secret value.
    UsageError::CredentialStorage(e.to_string())
}

impl CredentialStore for KeyringCredentialStore {
    fn set(&self, key: &CredentialKey, secret: &Secret) -> Result<()> {
        self.entry(key)?.set_password(secret.expose()).map_err(map_err)
    }
    fn get(&self, key: &CredentialKey) -> Result<Option<Secret>> {
        match self.entry(key)?.get_password() {
            Ok(p) => Ok(Some(Secret::new(p))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }
    fn delete(&self, key: &CredentialKey) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires a real OS keyring (Windows Credential Manager / macOS Keychain / a running
    /// Secret Service). Run with `cargo test -p usage-core -- --ignored keyring`.
    #[test]
    #[ignore = "needs an OS credential store; run on each target platform"]
    fn keyring_store_conforms_on_this_platform() {
        let store = KeyringCredentialStore::new("com.usagemonitormaximus.app.test");
        conformance_suite(&store, &CredentialKey::new("test-provider", "conformance"));
    }
}
