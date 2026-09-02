use serde_json::{Map, Value};

/// Overlays `overrides` onto `base`, one key at a time.
///
/// Nested objects merge rather than replace, so a settings file that names a
/// single key keeps every other value rather than resetting it. Arrays and
/// scalars replace outright: a partial array has no sensible meaning.
pub fn merge(base: Value, overrides: Value) -> Value {
    match (base, overrides) {
        (Value::Object(base), Value::Object(overrides)) => {
            let mut merged: Map<String, Value> = base;
            for (key, value) in overrides {
                let existing = merged.remove(&key).unwrap_or(Value::Null);
                merged.insert(key, merge(existing, value));
            }
            Value::Object(merged)
        }
        // An explicit null in a stored document means "unset", not "override
        // with nothing", so the default survives.
        (base, Value::Null) => base,
        (_, overrides) => overrides,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn keys_absent_from_the_stored_document_keep_their_default() {
        let merged = merge(json!({ "a": 1, "b": 2 }), json!({ "b": 9 }));

        assert_eq!(merged, json!({ "a": 1, "b": 9 }));
    }

    #[test]
    fn nested_objects_merge_rather_than_replace() {
        let merged = merge(json!({ "outer": { "a": 1, "b": 2 } }), json!({ "outer": { "b": 9 } }));

        assert_eq!(merged, json!({ "outer": { "a": 1, "b": 9 } }));
    }

    #[test]
    fn arrays_replace_because_a_partial_array_means_nothing() {
        let merged = merge(json!({ "items": [1, 2, 3] }), json!({ "items": [9] }));

        assert_eq!(merged, json!({ "items": [9] }));
    }

    #[test]
    fn an_explicit_null_leaves_the_default_in_place() {
        let merged = merge(json!({ "a": 1 }), json!({ "a": null }));

        assert_eq!(merged, json!({ "a": 1 }));
    }

    #[test]
    fn an_unknown_key_is_carried_through_for_the_deserializer_to_reject_or_ignore() {
        let merged = merge(json!({ "a": 1 }), json!({ "z": 5 }));

        assert_eq!(merged, json!({ "a": 1, "z": 5 }));
    }
}
