//! Walks a `serde_yaml::Value` tree according to parsed path segments to
//! implement `get`/`set`/`delete`. Every failure mode (missing key, index
//! out of bounds, indexing into the wrong shape) returns a `OpError` with a
//! human-readable message instead of panicking or silently doing nothing.

use serde_yaml::mapping::Entry;
use serde_yaml::{Mapping, Value};
use std::fmt;

use crate::path::Segment;

#[derive(Debug, PartialEq, Eq)]
pub struct OpError(pub String);

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for OpError {}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Sequence(_) => "array",
        Value::Mapping(_) => "map",
        Value::Tagged(_) => "tagged value",
    }
}

/// Reads the value at `segments` under `root`. An empty segment list
/// (path `.`) returns the whole document.
pub fn get<'a>(root: &'a Value, segments: &[Segment]) -> Result<&'a Value, OpError> {
    let mut current = root;
    for seg in segments {
        current = match (current, seg) {
            (Value::Mapping(map), Segment::Key(k)) => map
                .get(k.as_str())
                .ok_or_else(|| OpError(format!("key '{k}' not found")))?,
            (Value::Sequence(seq), Segment::Index(i)) => seq.get(*i).ok_or_else(|| {
                OpError(format!(
                    "index {i} out of bounds (array has {} element(s))",
                    seq.len()
                ))
            })?,
            (other, seg) => {
                return Err(OpError(format!(
                    "cannot look up {seg} in a {} value",
                    type_name(other)
                )));
            }
        };
    }
    Ok(current)
}

fn get_mut<'a>(root: &'a mut Value, segments: &[Segment]) -> Result<&'a mut Value, OpError> {
    let mut current = root;
    for seg in segments {
        current = match (current, seg) {
            (Value::Mapping(map), Segment::Key(k)) => map
                .get_mut(k.as_str())
                .ok_or_else(|| OpError(format!("key '{k}' not found")))?,
            (Value::Sequence(seq), Segment::Index(i)) => {
                let len = seq.len();
                seq.get_mut(*i).ok_or_else(|| {
                    OpError(format!(
                        "index {i} out of bounds (array has {len} element(s))"
                    ))
                })?
            }
            (other, seg) => {
                return Err(OpError(format!(
                    "cannot look up {seg} in a {} value",
                    type_name(other)
                )));
            }
        };
    }
    Ok(current)
}

/// Sets the value at `segments` under `root` to `new_value`.
///
/// Missing **map** keys along the path are auto-created (a `null`/missing
/// node becomes an empty map when a key is being set under it) so that
/// `set '.a.b.c' 1` works against `{}`. Arrays are **not** auto-extended or
/// auto-created — indexing into a missing or out-of-bounds array index is
/// a clean error, since there's no sane default length to grow to.
pub fn set(root: &mut Value, segments: &[Segment], new_value: Value) -> Result<(), OpError> {
    let Some((head, rest)) = segments.split_first() else {
        *root = new_value;
        return Ok(());
    };

    match head {
        Segment::Key(k) => {
            if matches!(root, Value::Null) {
                *root = Value::Mapping(Mapping::new());
            }
            let Value::Mapping(map) = root else {
                return Err(OpError(format!(
                    "cannot set key '{k}' — value is a {}, not a map",
                    type_name(root)
                )));
            };
            if rest.is_empty() {
                map.insert(Value::String(k.clone()), new_value);
                return Ok(());
            }
            let entry_value = match map.entry(Value::String(k.clone())) {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => v.insert(Value::Null),
            };
            set(entry_value, rest, new_value)
        }
        Segment::Index(i) => {
            let Value::Sequence(seq) = root else {
                return Err(OpError(format!(
                    "cannot set index {i} — value is a {}, not an array (yamlq does not auto-create arrays)",
                    type_name(root)
                )));
            };
            if *i >= seq.len() {
                return Err(OpError(format!(
                    "index {i} out of bounds (array has {} element(s)) — yamlq does not auto-extend arrays",
                    seq.len()
                )));
            }
            if rest.is_empty() {
                seq[*i] = new_value;
                return Ok(());
            }
            set(&mut seq[*i], rest, new_value)
        }
    }
}

/// Deletes the value at `segments` under `root`. The path must resolve to
/// an existing key/index — deleting something that isn't there is a clean
/// error, not a silent no-op.
pub fn delete(root: &mut Value, segments: &[Segment]) -> Result<(), OpError> {
    let Some((last, ancestors)) = segments.split_last() else {
        return Err(OpError(
            "cannot delete the whole document — path must select something inside it".into(),
        ));
    };
    let parent = get_mut(root, ancestors)?;
    match (parent, last) {
        (Value::Mapping(map), Segment::Key(k)) => {
            map.remove(k.as_str())
                .ok_or_else(|| OpError(format!("key '{k}' not found, nothing to delete")))?;
            Ok(())
        }
        (Value::Sequence(seq), Segment::Index(i)) => {
            if *i >= seq.len() {
                return Err(OpError(format!(
                    "index {i} out of bounds (array has {} element(s)), nothing to delete",
                    seq.len()
                )));
            }
            seq.remove(*i);
            Ok(())
        }
        (other, seg) => Err(OpError(format!(
            "cannot delete {seg} from a {} value",
            type_name(other)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::parse;

    fn doc(yaml: &str) -> Value {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn get_nested_scalar() {
        let d = doc("a:\n  b:\n    c: 42\n");
        let v = get(&d, &parse(".a.b.c").unwrap()).unwrap();
        assert_eq!(v, &Value::Number(42.into()));
    }

    #[test]
    fn get_array_index() {
        let d = doc("items:\n  - one\n  - two\n  - three\n");
        let v = get(&d, &parse(".items[1]").unwrap()).unwrap();
        assert_eq!(v.as_str(), Some("two"));
    }

    #[test]
    fn get_root_returns_whole_doc() {
        let d = doc("a: 1\n");
        let v = get(&d, &parse(".").unwrap()).unwrap();
        assert_eq!(v, &d);
    }

    #[test]
    fn get_missing_key_is_clean_error_not_panic() {
        let d = doc("a: 1\n");
        let err = get(&d, &parse(".missing").unwrap()).unwrap_err();
        assert!(err.0.contains("not found"));
    }

    #[test]
    fn get_index_out_of_bounds_is_clean_error() {
        let d = doc("items:\n  - one\n");
        let err = get(&d, &parse(".items[5]").unwrap()).unwrap_err();
        assert!(err.0.contains("out of bounds"));
    }

    #[test]
    fn get_key_into_scalar_is_clean_error() {
        let d = doc("a: 1\n");
        let err = get(&d, &parse(".a.b").unwrap()).unwrap_err();
        assert!(err.0.contains("number"));
    }

    #[test]
    fn set_existing_scalar() {
        let mut d = doc("a:\n  b: 1\n");
        set(&mut d, &parse(".a.b").unwrap(), Value::Number(2.into())).unwrap();
        assert_eq!(get(&d, &parse(".a.b").unwrap()).unwrap().as_i64(), Some(2));
    }

    #[test]
    fn set_creates_missing_intermediate_maps() {
        let mut d = doc("{}\n");
        set(
            &mut d,
            &parse(".a.b.c").unwrap(),
            Value::String("deep".into()),
        )
        .unwrap();
        assert_eq!(
            get(&d, &parse(".a.b.c").unwrap()).unwrap().as_str(),
            Some("deep")
        );
    }

    #[test]
    fn set_array_index_in_place() {
        let mut d = doc("items:\n  - one\n  - two\n");
        set(
            &mut d,
            &parse(".items[0]").unwrap(),
            Value::String("ONE".into()),
        )
        .unwrap();
        assert_eq!(
            get(&d, &parse(".items[0]").unwrap()).unwrap().as_str(),
            Some("ONE")
        );
        // untouched sibling stays untouched
        assert_eq!(
            get(&d, &parse(".items[1]").unwrap()).unwrap().as_str(),
            Some("two")
        );
    }

    #[test]
    fn set_array_out_of_bounds_is_clean_error_no_autoextend() {
        let mut d = doc("items:\n  - one\n");
        let err = set(
            &mut d,
            &parse(".items[3]").unwrap(),
            Value::String("x".into()),
        )
        .unwrap_err();
        assert!(err.0.contains("out of bounds"));
    }

    #[test]
    fn set_whole_document_with_root_path() {
        let mut d = doc("a: 1\n");
        set(
            &mut d,
            &parse(".").unwrap(),
            Value::String("replaced".into()),
        )
        .unwrap();
        assert_eq!(d.as_str(), Some("replaced"));
    }

    #[test]
    fn delete_existing_key() {
        let mut d = doc("a: 1\nb: 2\n");
        delete(&mut d, &parse(".a").unwrap()).unwrap();
        assert!(get(&d, &parse(".a").unwrap()).is_err());
        assert_eq!(get(&d, &parse(".b").unwrap()).unwrap().as_i64(), Some(2));
    }

    #[test]
    fn delete_array_index_shifts_remaining_elements() {
        let mut d = doc("items:\n  - one\n  - two\n  - three\n");
        delete(&mut d, &parse(".items[0]").unwrap()).unwrap();
        assert_eq!(
            get(&d, &parse(".items[0]").unwrap()).unwrap().as_str(),
            Some("two")
        );
        let items = get(&d, &parse(".items").unwrap()).unwrap();
        assert_eq!(items.as_sequence().unwrap().len(), 2);
    }

    #[test]
    fn delete_missing_key_is_clean_error_not_silent_noop() {
        let mut d = doc("a: 1\n");
        let err = delete(&mut d, &parse(".missing").unwrap()).unwrap_err();
        assert!(err.0.contains("nothing to delete"));
    }

    #[test]
    fn delete_root_is_rejected() {
        let mut d = doc("a: 1\n");
        let err = delete(&mut d, &parse(".").unwrap()).unwrap_err();
        assert!(err.0.contains("whole document"));
    }
}
