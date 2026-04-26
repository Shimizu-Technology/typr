use keyring::Entry;

const SERVICE_NAME: &str = "com.typr.app";

#[derive(Clone, Copy)]
pub enum SecretKind {
    GroqApiKey,
    TransformApiKey,
}

#[derive(Clone, serde::Serialize)]
pub struct SecretStatus {
    #[serde(rename = "hasGroqApiKey")]
    pub has_groq_api_key: bool,
    #[serde(rename = "hasTransformApiKey")]
    pub has_transform_api_key: bool,
}

pub fn get_secret(kind: SecretKind) -> Result<Option<String>, String> {
    let entry = entry_for(kind)?;

    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(format_keyring_error("read secret", err)),
    }
}

pub fn get_nonempty_secret(kind: SecretKind) -> Result<Option<String>, String> {
    Ok(get_secret(kind)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

pub fn set_secret(kind: SecretKind, value: &str) -> Result<(), String> {
    let entry = entry_for(kind)?;
    entry
        .set_password(value)
        .map_err(|err| format_keyring_error("store secret", err))
}

pub fn delete_secret(kind: SecretKind) -> Result<(), String> {
    let entry = entry_for(kind)?;

    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format_keyring_error("delete secret", err)),
    }
}

pub fn get_secret_status() -> Result<SecretStatus, String> {
    Ok(SecretStatus {
        has_groq_api_key: get_nonempty_secret(SecretKind::GroqApiKey)?.is_some(),
        has_transform_api_key: get_nonempty_secret(SecretKind::TransformApiKey)?.is_some(),
    })
}

pub fn parse_secret_kind(kind: &str) -> Result<SecretKind, String> {
    match kind {
        "groq" => Ok(SecretKind::GroqApiKey),
        "transform" => Ok(SecretKind::TransformApiKey),
        other => Err(format!("Unknown secret kind: {}", other)),
    }
}

fn entry_for(kind: SecretKind) -> Result<Entry, String> {
    Entry::new(SERVICE_NAME, kind.user_name())
        .map_err(|err| format_keyring_error("initialize secure storage", err))
}

fn format_keyring_error(action: &str, err: keyring::Error) -> String {
    format!("Failed to {}: {}", action, err)
}

impl SecretKind {
    fn user_name(self) -> &'static str {
        match self {
            SecretKind::GroqApiKey => "groq_api_key",
            SecretKind::TransformApiKey => "transform_api_key",
        }
    }
}
