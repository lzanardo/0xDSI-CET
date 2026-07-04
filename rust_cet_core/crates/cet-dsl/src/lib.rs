//! # cet-dsl
//!
//! Parser for the compact CSV pattern strings accepted by the C engine's
//! `cet_parse_query` (`c_engine/src/dsl.c`).
//!
//! Grammar (informal):
//!
//! ```text
//! pattern := token ("," token)*
//! token   := IDENT "+"?           // "+" marks a Kleene-plus step
//! IDENT   := [A-Za-z_][A-Za-z0-9_]*
//! ```
//!
//! Whitespace around commas and tokens is ignored. Unlike the C parser, this
//! implementation is reentrant (no `strtok`), returns a typed error on
//! malformed input, and refuses to silently truncate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use cet_core::{EventType, Query};

/// Parse errors produced by [`parse_query`].
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum ParseError {
    /// The pattern string was empty or contained no non-empty tokens.
    #[error("empty pattern")]
    Empty,
    /// A token did not match the identifier grammar.
    #[error("invalid token at position {position}: {token:?}")]
    InvalidToken {
        /// Zero-based comma-separated token index.
        position: usize,
        /// The offending raw token.
        token: String,
    },
    /// The pattern exceeded [`cet_core::caps::MAX_SEQ`] tokens.
    #[error("pattern exceeded MAX_SEQ ({max}) tokens")]
    TooManyTokens {
        /// The maximum permitted length.
        max: usize,
    },
}

/// Parse a CSV pattern string into a [`Query`].
///
/// # Examples
///
/// ```
/// let q = cet_dsl::parse_query("q", "A+,B,C", 60_000, 10_000).unwrap();
/// assert_eq!(q.seq.len(), 3);
/// assert!(q.seq[0].kleene_plus);
/// assert_eq!(q.seq[1].name, "B");
/// ```
pub fn parse_query(
    name: &str,
    pattern_csv: &str,
    within_ms: i64,
    slide_ms: i64,
) -> Result<Query, ParseError> {
    let mut seq: Vec<EventType> = Vec::new();
    for (pos, raw) in pattern_csv.split(',').enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (name_part, kleene_plus) = if let Some(stripped) = trimmed.strip_suffix('+') {
            (stripped.trim(), true)
        } else {
            (trimmed, false)
        };
        if !is_ident(name_part) {
            return Err(ParseError::InvalidToken { position: pos, token: raw.to_string() });
        }
        if seq.len() >= cet_core::caps::MAX_SEQ {
            return Err(ParseError::TooManyTokens { max: cet_core::caps::MAX_SEQ });
        }
        seq.push(EventType { name: name_part.to_string(), kleene_plus, predicate: None });
    }
    if seq.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut q = Query::new(name, seq);
    q.within_ms = within_ms;
    q.slide_ms = slide_ms;
    Ok(q)
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn simple_pattern() {
        let q = parse_query("q", "A+,B,C", 60_000, 10_000).unwrap();
        assert_eq!(q.name, "q");
        assert_eq!(q.within_ms, 60_000);
        assert_eq!(q.slide_ms, 10_000);
        assert_eq!(q.seq.len(), 3);
        assert!(q.seq[0].kleene_plus);
        assert!(!q.seq[1].kleene_plus);
        assert_eq!(q.seq[2].name, "C");
    }

    #[rstest]
    #[case(" A , B , C ")]
    #[case("A,B,C")]
    #[case("A, B ,C")]
    fn whitespace_insensitive(#[case] pattern: &str) {
        let q = parse_query("q", pattern, 0, 0).unwrap();
        let names: Vec<_> = q.seq.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn empty_pattern_rejected() {
        assert_eq!(parse_query("q", "", 0, 0), Err(ParseError::Empty));
        assert_eq!(parse_query("q", "   ,  , ", 0, 0), Err(ParseError::Empty));
    }

    #[test]
    fn invalid_token_rejected() {
        let err = parse_query("q", "A,1B,C", 0, 0).unwrap_err();
        assert!(matches!(err, ParseError::InvalidToken { position: 1, .. }));
    }
}
