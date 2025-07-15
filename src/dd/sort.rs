use std::borrow::Cow;

use regex::Regex;
use serde_json::Value;

/// Returns a deep-sorted copy of the [`serde_json::Value`]
#[cfg(test)]
pub fn sort_value(v: &Value, ignore_keys: &[Regex]) -> Value {
    match v {
        Value::Array(a) => Value::Array(
            preprocess_array(
                true,
                &a.iter().map(|e| sort_value(e, ignore_keys)).collect(),
                ignore_keys,
            )
            .into_owned(),
        ),
        Value::Object(a) => Value::Object(
            a.iter()
                .map(|(k, v)| (k.clone(), sort_value(v, ignore_keys)))
                .collect(),
        ),
        v => v.clone(),
    }
}

pub(crate) fn preprocess_array<'a>(
    sort_arrays: bool,
    a: &'a Vec<Value>,
    ignore_keys: &[Regex],
) -> Cow<'a, Vec<Value>> {
    if sort_arrays || !ignore_keys.is_empty() {
        let mut owned = a.to_owned();
        owned.sort_by(|a, b| compare_values(a, b, ignore_keys));
        Cow::Owned(owned)
    } else {
        Cow::Borrowed(a)
    }
}
fn compare_values(a: &Value, b: &Value, ignore_keys: &[Regex]) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Number(a), Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                return a.cmp(&b);
            }
            if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) {
                return a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
            }
            // Handle other number types if needed
            std::cmp::Ordering::Equal
        }
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Array(a), Value::Array(b)) => {
            let a = preprocess_array(true, a, ignore_keys);
            let b = preprocess_array(true, b, ignore_keys);
            for (a, b) in a.iter().zip(b.iter()) {
                let cmp = compare_values(a, b, ignore_keys);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            a.len().cmp(&b.len())
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut keys_a: Vec<_> = a.keys().collect();
            let mut keys_b: Vec<_> = b.keys().collect();
            keys_a.sort();
            keys_b.sort();
            for (key_a, key_b) in keys_a
                .iter()
                .filter(|a| ignore_keys.iter().all(|r| !r.is_match(a)))
                .zip(
                    keys_b
                        .iter()
                        .filter(|a| ignore_keys.iter().all(|r| !r.is_match(a))),
                )
            {
                let cmp = key_a.cmp(key_b);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
                let value_a = &a[*key_a];
                let value_b = &b[*key_b];
                let cmp = compare_values(value_a, value_b, ignore_keys);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            keys_a.len().cmp(&keys_b.len())
        }
        (Value::Object(_), _) => std::cmp::Ordering::Less,
        (_, Value::Object(_)) => std::cmp::Ordering::Greater,
        (Value::Bool(_), _) => std::cmp::Ordering::Less,
        (_, Value::Bool(_)) => std::cmp::Ordering::Greater,
        (Value::Number(_), _) => std::cmp::Ordering::Less,
        (_, Value::Number(_)) => std::cmp::Ordering::Greater,
        (Value::String(_), _) => std::cmp::Ordering::Less,
        (_, Value::String(_)) => std::cmp::Ordering::Greater,
    }
}
