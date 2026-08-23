//! The domain: what a short link *is*, independent of HTTP or AT Protocol.
//!
//! Pure, no I/O, and depends on nothing but `error` for the conversions at the
//! bottom. Everything here exists so that a validated value and an unvalidated
//! one stop sharing a type.
//!
//! The motivating defect: `is_allowed_scheme` was a free function that the write
//! path remembered to call and the read path did not. Anyone could `putRecord` a
//! `javascript:` URL straight to their own PDS, and it would be handed to
//! `Redirect::temporary` and rendered into an `<a href>` on the info page —
//! Askama escapes characters, not schemes. Now the only way to obtain a
//! `TargetUrl` is through a constructor that enforces the scheme, and both paths
//! carry `TargetUrl` rather than `String`.

use rand::Rng;

use crate::error::AppError;

/// Characters a generated short code is drawn from.
const CODE_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Length of a generated short code.
const GENERATED_CODE_LEN: usize = 6;

/// Why a short code was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvalidShortCode {
    /// The code was empty.
    #[error("short code must not be empty")]
    Empty,
    /// The code exceeded [`ShortCode::MAX_LEN`].
    #[error("short code must be at most 64 characters")]
    TooLong,
    /// The code contained a character outside `[A-Za-z0-9_-]`.
    #[error("short code may only contain letters, digits, '-' and '_'")]
    IllegalCharacter,
}

/// A validated short code: 1–64 characters of `[A-Za-z0-9_-]`.
///
/// This charset is a strict subset of the AT Protocol record-key charset
/// (`[A-Za-z0-9._:~-]`, 1–512 bytes), so a `ShortCode` is always a valid rkey.
/// `test_short_code_is_always_a_valid_rkey` holds that assumption in place.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShortCode(String);

impl ShortCode {
    /// Maximum length of a short code.
    pub const MAX_LEN: usize = 64;

    /// Validate a user-supplied short code.
    pub fn parse(s: &str) -> Result<Self, InvalidShortCode> {
        if s.is_empty() {
            return Err(InvalidShortCode::Empty);
        }
        if s.len() > Self::MAX_LEN {
            return Err(InvalidShortCode::TooLong);
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(InvalidShortCode::IllegalCharacter);
        }
        Ok(Self(s.to_string()))
    }

    /// Generate a random short code.
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let s = (0..GENERATED_CODE_LEN)
            .map(|_| CODE_CHARSET[rng.random_range(0..CODE_CHARSET.len())] as char)
            .collect();
        Self(s)
    }

    /// The code as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ShortCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a destination URL was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvalidTarget {
    /// The string did not parse as an absolute URL.
    #[error("destination is not a valid URL")]
    NotAUrl,
    /// The URL exceeded [`TargetUrl::MAX_LEN`].
    #[error("destination URL is too long")]
    TooLong,
    /// The scheme was something other than `http` or `https`.
    #[error("only http and https destinations are allowed")]
    DisallowedScheme,
}

/// A destination URL that is safe to redirect a browser to.
///
/// Guarantees: parses as an absolute URL, scheme is `http` or `https`, and the
/// serialized form is at most [`TargetUrl::MAX_LEN`] bytes. Because this is the
/// only constructor, holding a `TargetUrl` *is* the proof those checks ran —
/// which is what stops a `javascript:` record reaching an `href`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetUrl(url::Url);

impl TargetUrl {
    /// Maximum destination length.
    ///
    /// Mirrors `maxLength` on the `url` property in `lexicons/to/atpr/link.json`;
    /// `test_max_target_len_matches_lexicon` fails the build if they drift.
    pub const MAX_LEN: usize = 2048;

    /// Validate a destination URL.
    pub fn parse(s: &str) -> Result<Self, InvalidTarget> {
        if s.len() > Self::MAX_LEN {
            return Err(InvalidTarget::TooLong);
        }
        let url = url::Url::parse(s).map_err(|_| InvalidTarget::NotAUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(InvalidTarget::DisallowedScheme);
        }
        Ok(Self(url))
    }

    /// The destination as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The underlying parsed URL.
    pub fn url(&self) -> &url::Url {
        &self.0
    }
}

impl std::fmt::Display for TargetUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

/// A short link as stored in a repo: where it points, and when it last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortLink {
    /// Where the link points.
    pub target: TargetUrl,
    /// Last-modified timestamp, as the ISO 8601 string the record carries.
    pub updated_at: Option<String>,
}

impl From<InvalidShortCode> for AppError {
    fn from(e: InvalidShortCode) -> Self {
        // Every variant has a fixed, server-authored message, so this stays
        // inside `BadRequest`'s `&'static str` contract.
        AppError::BadRequest(match e {
            InvalidShortCode::Empty => "short code must not be empty",
            InvalidShortCode::TooLong => "short code must be at most 64 characters",
            InvalidShortCode::IllegalCharacter => {
                "short code may only contain letters, digits, '-' and '_'"
            }
        })
    }
}

impl From<InvalidTarget> for AppError {
    fn from(e: InvalidTarget) -> Self {
        AppError::BadRequest(match e {
            InvalidTarget::NotAUrl => "destination is not a valid URL",
            InvalidTarget::TooLong => "destination URL is too long (max 2048 characters)",
            InvalidTarget::DisallowedScheme => "only http and https destinations are allowed",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_accepts_valid_codes() {
        for s in ["abc123", "my-code", "my_code", "A", &"a".repeat(64)] {
            assert!(ShortCode::parse(s).is_ok(), "should accept {s:?}");
        }
    }

    #[test]
    fn test_parse_rejects_invalid_codes() {
        assert_eq!(ShortCode::parse(""), Err(InvalidShortCode::Empty));
        assert_eq!(
            ShortCode::parse(&"a".repeat(65)),
            Err(InvalidShortCode::TooLong)
        );
        for s in [
            "has spaces",
            "has/slash",
            "has.dot",
            "emoji🎉",
            "semi;colon",
        ] {
            assert_eq!(
                ShortCode::parse(s),
                Err(InvalidShortCode::IllegalCharacter),
                "should reject {s:?}"
            );
        }
    }

    /// A path separator in a code would let it escape its route segment.
    #[test]
    fn test_parse_rejects_path_traversal() {
        assert!(ShortCode::parse("../../etc/passwd").is_err());
        assert!(ShortCode::parse("a/b").is_err());
    }

    #[test]
    fn test_generate_is_valid_and_right_length() {
        for _ in 0..200 {
            let code = ShortCode::generate();
            assert_eq!(code.as_str().len(), GENERATED_CODE_LEN);
            assert_eq!(ShortCode::parse(code.as_str()).unwrap(), code);
        }
    }

    /// `ShortCode`'s charset must remain a subset of the atproto record-key
    /// charset, because the write path relies on that to build an rkey.
    #[test]
    fn test_short_code_is_always_a_valid_rkey() {
        use jacquard_common::types::recordkey::{RecordKey, Rkey};

        let mut samples = vec![
            "a".to_string(),
            "abc123".to_string(),
            "my-code".to_string(),
            "my_code".to_string(),
            "a".repeat(ShortCode::MAX_LEN),
        ];
        for _ in 0..50 {
            samples.push(ShortCode::generate().as_str().to_string());
        }

        for s in samples {
            let code = ShortCode::parse(&s).expect("sample is a valid short code");
            assert!(
                RecordKey::<Rkey>::any_owned(code.as_str()).is_ok(),
                "ShortCode {:?} is not a valid rkey",
                code.as_str()
            );
        }
    }

    #[test]
    fn test_target_accepts_http_and_https() {
        assert!(TargetUrl::parse("https://example.com").is_ok());
        assert!(TargetUrl::parse("http://example.com/path?q=1#frag").is_ok());
    }

    /// The reason this type exists.
    #[test]
    fn test_target_rejects_dangerous_schemes() {
        for s in [
            "javascript:alert(1)",
            "javascript:void(0)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "file:///etc/passwd",
            "ftp://example.com/file.txt",
        ] {
            assert_eq!(
                TargetUrl::parse(s),
                Err(InvalidTarget::DisallowedScheme),
                "should reject {s:?}"
            );
        }
    }

    /// Scheme matching is case-insensitive in URL parsing, so an uppercased
    /// scheme must not slip past a lowercase comparison.
    #[test]
    fn test_target_rejects_uppercased_dangerous_scheme() {
        assert_eq!(
            TargetUrl::parse("JavaScript:alert(1)"),
            Err(InvalidTarget::DisallowedScheme)
        );
        assert!(TargetUrl::parse("HTTPS://example.com").is_ok());
    }

    #[test]
    fn test_target_rejects_relative_and_garbage() {
        assert_eq!(
            TargetUrl::parse("/just/a/path"),
            Err(InvalidTarget::NotAUrl)
        );
        assert_eq!(
            TargetUrl::parse("not-a-valid-url"),
            Err(InvalidTarget::NotAUrl)
        );
        assert_eq!(TargetUrl::parse(""), Err(InvalidTarget::NotAUrl));
    }

    #[test]
    fn test_target_length_limit() {
        let long = format!("https://example.com/{}", "a".repeat(TargetUrl::MAX_LEN));
        assert_eq!(TargetUrl::parse(&long), Err(InvalidTarget::TooLong));

        let ok = format!(
            "https://example.com/{}",
            "a".repeat(TargetUrl::MAX_LEN - "https://example.com/".len())
        );
        assert_eq!(ok.len(), TargetUrl::MAX_LEN);
        assert!(TargetUrl::parse(&ok).is_ok());
    }

    /// The `2048` in this module must not drift from the lexicon that actually
    /// governs what a PDS will accept. `include_str!` keeps the check hermetic:
    /// it resolves at compile time, so the test does not depend on the working
    /// directory.
    #[test]
    fn test_max_target_len_matches_lexicon() {
        const LEXICON: &str = include_str!("../../../lexicons/to/atpr/link.json");
        let doc: serde_json::Value = serde_json::from_str(LEXICON).expect("lexicon is valid JSON");

        let max_length = doc["defs"]["main"]["record"]["properties"]["url"]["maxLength"]
            .as_u64()
            .expect("lexicon declares url.maxLength");

        assert_eq!(
            max_length as usize,
            TargetUrl::MAX_LEN,
            "TargetUrl::MAX_LEN has drifted from lexicons/to/atpr/link.json"
        );
    }

    #[test]
    fn test_error_conversions_are_bad_requests() {
        use axum::http::StatusCode;
        assert_eq!(
            AppError::from(InvalidShortCode::Empty).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::from(InvalidTarget::DisallowedScheme).status(),
            StatusCode::BAD_REQUEST
        );
    }
}
