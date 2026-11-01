//! Redaction and merge helpers for JSON5/JSON documents served through the
//! Config API and FS API.
//!
//! The server never returns real secret values: exact secret-key matches are
//! replaced by `"***"` in parsed documents. On write, a parsed document may
//! contain `"***"` or `"<redacted>"` as a secret value; in that case the
//! previous value from the existing parsed document is restored. Callers are
//! responsible for parsing JSON5/JSON and for deciding when merge may run.

use serde_json::{Map, Value};

/// Keys whose values are secret and must be redacted on read.
pub const SECRET_KEYS: &[&str] = &[
    "api-key",
    "apikey",
    "token",
    "secret",
    "private-key",
    "tls-key",
];

/// Values that mean "keep the previous secret" during a write.
pub const REDACTION_MARKERS: &[&str] = &["***", "<redacted>"];

/// `true` when `key` is an exact secret key (no substring matching).
pub fn is_secret_key(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

/// `true` when `value` is a string equal to a redaction marker.
pub fn is_redaction_marker(value: &Value) -> bool {
    value
        .as_str()
        .map(|text| REDACTION_MARKERS.contains(&text))
        .unwrap_or(false)
}

/// Recursively replace secret values with `"***"`.
pub fn redact(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut redacted = Map::new();
            for (key, value) in object {
                if is_secret_key(key) {
                    redacted.insert(key.clone(), Value::String("***".to_string()));
                } else {
                    redacted.insert(key.clone(), redact(value));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact).collect()),
        other => other.clone(),
    }
}

/// `true` when the document contains a redaction marker for a secret key.
pub fn has_redacted_secret(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (is_secret_key(key) && is_redaction_marker(value)) || has_redacted_secret(value)
        }),
        Value::Array(items) => items.iter().any(has_redacted_secret),
        _ => false,
    }
}

/// Restore previous secret values for redaction markers in `value`.
///
/// Traverses the incoming document and, for each secret key whose value is a
/// redaction marker, copies the value from the same path in `existing`. The
/// function fails when the previous value is absent or is not a string, so a
/// marker can never be persisted as a real secret value.
pub fn merge_redacted_secrets(value: &mut Value, existing: &Value) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                if is_secret_key(key) {
                    if is_redaction_marker(value) {
                        let previous = existing.get(key.as_str()).ok_or_else(|| {
                            format!("cannot merge secret '{key}': previous value is missing")
                        })?;
                        if !previous.is_string() {
                            return Err(format!(
                                "cannot merge secret '{key}': previous value is not a string"
                            ));
                        }
                        *value = previous.clone();
                    }
                } else {
                    let previous_child = existing.get(key.as_str()).unwrap_or(&Value::Null);
                    merge_redacted_secrets(value, previous_child)?;
                }
            }
        }
        Value::Array(items) => {
            let previous_items = existing.as_array();
            for (index, item) in items.iter_mut().enumerate() {
                let previous_child = previous_items
                    .and_then(|items| items.get(index))
                    .unwrap_or(&Value::Null);
                merge_redacted_secrets(item, previous_child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redact_replaces_exact_secret_keys_recursively() {
        let value = json!({
            "agent": { "api-key": "sk-123", "name": "DeepSeek" },
            "server": { "tls-key": "c:/secret.pem" },
            "not-secret": "api-key is here",
            "items": [{ "token": "abc", "value": "keep" }]
        });

        let redacted = redact(&value);
        assert_eq!(redacted["agent"]["api-key"], "***");
        assert_eq!(redacted["server"]["tls-key"], "***");
        assert_eq!(redacted["items"][0]["token"], "***");
        assert_eq!(redacted["not-secret"], "api-key is here");
        assert_eq!(redacted["agent"]["name"], "DeepSeek");
    }

    #[test]
    fn merge_restores_markers_from_existing_document() {
        let mut incoming = json!({
            "server": { "tls-key": "***" },
            "nested": { "token": "<redacted>" },
            "new-secret": "sk-actual"
        });
        let existing = json!({
            "server": { "tls-key": "c:/old.pem" },
            "nested": { "token": "old-token" }
        });

        merge_redacted_secrets(&mut incoming, &existing).unwrap();
        assert_eq!(incoming["server"]["tls-key"], "c:/old.pem");
        assert_eq!(incoming["nested"]["token"], "old-token");
        assert_eq!(incoming["new-secret"], "sk-actual");
    }

    #[test]
    fn merge_fails_when_previous_secret_is_missing_or_non_string() {
        let mut missing = json!({ "server": { "tls-key": "***" } });
        let existing = json!({ "server": {} });
        assert!(merge_redacted_secrets(&mut missing, &existing).is_err());

        let mut non_string = json!({ "server": { "tls-key": "***" } });
        let existing = json!({ "server": { "tls-key": 42 } });
        assert!(merge_redacted_secrets(&mut non_string, &existing).is_err());
    }
}
