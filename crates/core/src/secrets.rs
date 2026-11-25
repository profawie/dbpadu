//! Saved passwords.
//!
//! Passwords are kept in the operating system's credential store: Windows
//! Credential Manager, the macOS Keychain, or the Secret Service on Linux.
//! They are never written to a file of this app.

use uuid::Uuid;

/// Service name the entries are filed under in the credential store.
pub const SERVICE: &str = crate::APP_NAME;

/// Handle to the saved passwords, one per connection.
#[derive(Clone, Copy)]
pub struct Secrets {
    available: bool,
}

impl Secrets {
    /// The operating system's credential store, if this system has one.
    pub fn os() -> Self {
        Self {
            available: keyring::Entry::store_status().is_ok(),
        }
    }

    /// No store: nothing is saved and nothing is found.
    pub fn disabled() -> Self {
        Self { available: false }
    }

    /// Whether passwords can be saved on this system.
    pub fn available(&self) -> bool {
        self.available
    }

    fn entry(&self, connection: Uuid) -> Result<keyring::Entry, String> {
        if !self.available {
            return Err("This system has no credential store.".to_string());
        }
        keyring::Entry::new(SERVICE, &format!("password:{connection}"))
            .map_err(|err| err.to_string())
    }

    /// The saved password of a connection, if there is one.
    pub fn get(&self, connection: Uuid) -> Option<String> {
        self.entry(connection).ok()?.get_password().ok()
    }

    pub fn set(&self, connection: Uuid, password: &str) -> Result<(), String> {
        self.entry(connection)?
            .set_password(password)
            .map_err(|err| err.to_string())
    }

    /// Forget the password. Not an error if none was saved.
    pub fn delete(&self, connection: Uuid) {
        if let Ok(entry) = self.entry(connection) {
            let _ = entry.delete_credential();
        }
    }
}
