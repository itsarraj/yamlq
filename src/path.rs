//! Parses a `yq`-style dot/bracket path (`.a.b[0]["weird key"]`) into a flat
//! list of segments that `ops.rs` walks over a `serde_yaml::Value`.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// A map key, from `.name` or `["name"]`/`['name']`.
    Key(String),
    /// An array index, from `[0]`.
    Index(usize),
}

impl fmt::Display for Segment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Segment::Key(k) => write!(f, ".{k}"),
            Segment::Index(i) => write!(f, "[{i}]"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PathError(pub String);

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PathError {}

/// Parses a path like `.a.b[0].c` or `.a["weird.key"][2]` into segments.
/// `.` alone (or an empty string) means "the whole document" and parses to
/// an empty segment list.
pub fn parse(path: &str) -> Result<Vec<Segment>, PathError> {
    let chars: Vec<char> = path.chars().collect();
    let mut i = 0usize;
    let mut segments = Vec::new();

    // A bare "." or empty path means "root" — nothing to do.
    if path.is_empty() || path == "." {
        return Ok(segments);
    }

    while i < chars.len() {
        match chars[i] {
            '.' => {
                i += 1;
                if i >= chars.len() {
                    return Err(PathError("path ends with a trailing '.'".into()));
                }
                if chars[i] == '[' {
                    // ".[0]" style is allowed; the bracket parser below picks it up.
                    continue;
                }
                let start = i;
                while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                    i += 1;
                }
                if i == start {
                    return Err(PathError(format!(
                        "empty key at position {start} in path '{path}'"
                    )));
                }
                segments.push(Segment::Key(chars[start..i].iter().collect()));
            }
            '[' => {
                i += 1;
                if i >= chars.len() {
                    return Err(PathError("unclosed '[' in path".into()));
                }
                if chars[i] == '"' || chars[i] == '\'' {
                    let quote = chars[i];
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i] != quote {
                        i += 1;
                    }
                    if i >= chars.len() {
                        return Err(PathError("unterminated quoted key in path".into()));
                    }
                    let key: String = chars[start..i].iter().collect();
                    i += 1; // skip closing quote
                    if i >= chars.len() || chars[i] != ']' {
                        return Err(PathError("expected ']' after quoted key".into()));
                    }
                    i += 1; // skip ']'
                    segments.push(Segment::Key(key));
                } else {
                    let start = i;
                    while i < chars.len() && chars[i] != ']' {
                        i += 1;
                    }
                    if i >= chars.len() {
                        return Err(PathError("unclosed '[' in path".into()));
                    }
                    let raw: String = chars[start..i].iter().collect();
                    let idx: usize = raw.parse().map_err(|_| {
                        PathError(format!("'{raw}' inside [] is not a valid array index"))
                    })?;
                    i += 1; // skip ']'
                    segments.push(Segment::Index(idx));
                }
            }
            other => {
                return Err(PathError(format!(
                    "unexpected '{other}' at position {i} in path '{path}' — paths must start with '.' or '['"
                )));
            }
        }
    }

    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_path_is_empty_segments() {
        assert_eq!(parse(".").unwrap(), vec![]);
        assert_eq!(parse("").unwrap(), vec![]);
    }

    #[test]
    fn single_key() {
        assert_eq!(parse(".a").unwrap(), vec![Segment::Key("a".into())]);
    }

    #[test]
    fn nested_keys() {
        assert_eq!(
            parse(".a.b.c").unwrap(),
            vec![
                Segment::Key("a".into()),
                Segment::Key("b".into()),
                Segment::Key("c".into())
            ]
        );
    }

    #[test]
    fn key_then_index() {
        assert_eq!(
            parse(".a.b[0]").unwrap(),
            vec![
                Segment::Key("a".into()),
                Segment::Key("b".into()),
                Segment::Index(0)
            ]
        );
    }

    #[test]
    fn index_then_key() {
        assert_eq!(
            parse(".a[0].c").unwrap(),
            vec![
                Segment::Key("a".into()),
                Segment::Index(0),
                Segment::Key("c".into())
            ]
        );
    }

    #[test]
    fn quoted_key_with_dot_inside() {
        assert_eq!(
            parse(".a[\"weird.key\"]").unwrap(),
            vec![Segment::Key("a".into()), Segment::Key("weird.key".into())]
        );
    }

    #[test]
    fn single_quoted_key() {
        assert_eq!(
            parse(".a['weird key']").unwrap(),
            vec![Segment::Key("a".into()), Segment::Key("weird key".into())]
        );
    }

    #[test]
    fn leading_bracket_index() {
        assert_eq!(parse("[0]").unwrap(), vec![Segment::Index(0)]);
    }

    #[test]
    fn multiple_indices() {
        assert_eq!(
            parse(".matrix[1][2]").unwrap(),
            vec![
                Segment::Key("matrix".into()),
                Segment::Index(1),
                Segment::Index(2)
            ]
        );
    }

    #[test]
    fn trailing_dot_is_an_error() {
        assert!(parse(".a.").is_err());
    }

    #[test]
    fn unclosed_bracket_is_an_error() {
        assert!(parse(".a[0").is_err());
    }

    #[test]
    fn non_numeric_index_is_an_error() {
        assert!(parse(".a[x]").is_err());
    }

    #[test]
    fn missing_leading_dot_is_an_error() {
        assert!(parse("a.b").is_err());
    }
}
