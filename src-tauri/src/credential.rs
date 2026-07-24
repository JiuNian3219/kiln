use keyring::Entry;

const SERVICE: &str = "Codex Input Enhancer";
const ACCOUNT: &str = "deepseek-api-key";

/// The only module allowed to read or write the DeepSeek secret.
pub struct WindowsCredentialStore;

impl WindowsCredentialStore {
    fn entry() -> Result<Entry, String> {
        Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())
    }

    pub fn configured() -> bool {
        Self::entry()
            .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
            .is_ok()
    }

    /// Secrets are deliberately read immediately before an API request and are never cached.
    pub fn load() -> Result<String, String> {
        let value = Self::entry()?.get_password().map_err(|_| {
            "No DeepSeek API Key is saved. Open settings with Ctrl+Shift+Alt+S.".to_string()
        })?;
        if value.trim().is_empty() {
            return Err(
                "No DeepSeek API Key is saved. Open settings with Ctrl+Shift+Alt+S.".to_string(),
            );
        }
        Ok(value)
    }

    pub fn save(value: &str) -> Result<(), String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(());
        }
        let entry = Self::entry()?;
        entry
            .set_password(value)
            .map_err(|error| error.to_string())?;
        if entry.get_password().map_err(|error| error.to_string())? != value {
            return Err("API Key verification failed after saving.".to_string());
        }
        Ok(())
    }
}
