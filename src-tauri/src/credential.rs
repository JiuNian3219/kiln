use keyring::Entry;

const SERVICE: &str = "Codex Input Enhancer";
const LEGACY_ACCOUNT: &str = "deepseek-api-key";

/// The only module allowed to read or write model-service secrets.
pub struct WindowsCredentialStore;

impl WindowsCredentialStore {
    fn entry(account: &str) -> Result<Entry, String> {
        Entry::new(SERVICE, account).map_err(|error| error.to_string())
    }

    fn account_for(provider_id: &str) -> Result<String, String> {
        if provider_id.is_empty()
            || !provider_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err("无效的 AI 服务标识。".to_string());
        }
        Ok(format!("model-provider-{provider_id}"))
    }

    pub fn configured_for(provider_id: &str) -> bool {
        let Ok(account) = Self::account_for(provider_id) else {
            return false;
        };
        Self::entry(&account)
            .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
            .is_ok()
            || (provider_id == "deepseek"
                && Self::entry(LEGACY_ACCOUNT)
                    .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
                    .is_ok())
    }

    /// Secrets are deliberately read immediately before an API request and are never cached.
    pub fn load_for(provider_id: &str) -> Result<String, String> {
        let account = Self::account_for(provider_id)?;
        let value = match Self::entry(&account)
            .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
        {
            Ok(value) => value,
            Err(_) if provider_id == "deepseek" => Self::entry(LEGACY_ACCOUNT)
                .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
                .map_err(|_| "未保存该 AI 服务的 API Key。请在控制面板中配置。".to_string())?,
            Err(_) => return Err("未保存该 AI 服务的 API Key。请在控制面板中配置。".to_string()),
        };
        if value.trim().is_empty() {
            return Err("未保存该 AI 服务的 API Key。请在控制面板中配置。".to_string());
        }
        Ok(value)
    }

    pub fn save_for(provider_id: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(());
        }
        let entry = Self::entry(&Self::account_for(provider_id)?)?;
        entry
            .set_password(value)
            .map_err(|error| error.to_string())?;
        if entry.get_password().map_err(|error| error.to_string())? != value {
            return Err("API Key verification failed after saving.".to_string());
        }
        Ok(())
    }

    pub fn delete_for(provider_id: &str) -> Result<(), String> {
        let entry = Self::entry(&Self::account_for(provider_id)?)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}
