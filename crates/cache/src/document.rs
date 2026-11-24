//! Reject ambiguous cache input documents before deserializing their declared types.
fn json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let value: serde_yaml_ng::Value =
        serde_json::from_slice(bytes).map_err(|error: serde_json::Error| {
            let message = error.to_string();
            if let Some((quoted, location)) = message
                .strip_prefix("duplicate entry with key ")
                .and_then(|tail| tail.rsplit_once(" at line "))
                && let Ok(key) = litrs::StringLit::parse(quoted)
            {
                return format!("duplicate JSON member {} at line {location}", key.value());
            }
            message
        })?;
    serde_json::to_value(value).map_err(|error| error.to_string())
}

pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_value(json(bytes)?).map_err(|e| e.to_string())
}
