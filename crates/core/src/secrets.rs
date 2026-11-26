//! Saved passwords.
//!
//! Passwords are kept in the operating system's credential store: Windows
//! Credential Manager, the macOS Keychain, or the Secret Service on Linux.
//! They are never written to a file of this app.

use uuid::Uuid;

/// Which secret of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    /// The database password.
    Password,
    /// The SSH password, or the passphrase of the SSH key.
    Ssh,
    /// The same for a jump server, named by its id.
    Jump(Uuid),
}

impl Secret {
    fn prefix(self) -> String {
        match self {
            Secret::Password => "password".to_string(),
            Secret::Ssh => "ssh".to_string(),
            Secret::Jump(jump) => format!("jump-{jump}"),
        }
    }
}

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

    fn entry(&self, connection: Uuid, secret: Secret) -> Result<keyring::Entry, String> {
        if !self.available {
            return Err("This system has no credential store.".to_string());
        }
        keyring::Entry::new(SERVICE, &format!("{}:{connection}", secret.prefix()))
            .map_err(|err| err.to_string())
    }

    /// A saved secret of a connection, if there is one.
    pub fn get(&self, connection: Uuid, secret: Secret) -> Option<String> {
        self.entry(connection, secret).ok()?.get_password().ok()
    }

    pub fn set(&self, connection: Uuid, secret: Secret, value: &str) -> Result<(), String> {
        self.entry(connection, secret)?
            .set_password(value)
            .map_err(|err| err.to_string())
    }

    /// Forget a secret. Not an error if none was saved.
    pub fn delete(&self, connection: Uuid, secret: Secret) {
        if let Ok(entry) = self.entry(connection, secret) {
            let _ = entry.delete_credential();
        }
    }
}
