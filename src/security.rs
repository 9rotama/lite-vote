//! Shared security primitives for opaque browser tokens and request origins.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http::HeaderMap;
use rand::{TryRngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

pub const TOKEN_BYTES: usize = 32;
pub const ENCODED_TOKEN_LENGTH: usize = 43;

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("secure random number generation failed")]
    Random,
}

pub fn random_token() -> Result<String, TokenError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| TokenError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[must_use]
pub fn hash_token(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

#[must_use]
pub fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let Some(host) = headers.get("host").and_then(|value| value.to_str().ok()) else {
        return false;
    };
    origin == format!("https://{host}") || origin == format!("http://{host}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_tokens_are_url_safe_and_have_256_bits() {
        let first = random_token().unwrap();
        let second = random_token().unwrap();
        assert_eq!(first.len(), ENCODED_TOKEN_LENGTH);
        assert_eq!(URL_SAFE_NO_PAD.decode(&first).unwrap().len(), TOKEN_BYTES);
        assert_ne!(first, second);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        );
    }

    #[test]
    fn token_hash_is_stable_and_does_not_reveal_the_token() {
        assert_eq!(hash_token("secret"), hash_token("secret"));
        assert_ne!(hash_token("secret"), hash_token("different"));
        assert!(!hash_token("secret").contains("secret"));
    }

    #[test]
    fn origin_must_exactly_match_the_http_or_https_host() {
        let headers = |origin: Option<&str>, host: Option<&str>| {
            let mut headers = HeaderMap::new();
            if let Some(origin) = origin {
                headers.insert("origin", origin.parse().unwrap());
            }
            if let Some(host) = host {
                headers.insert("host", host.parse().unwrap());
            }
            headers
        };
        assert!(same_origin(&headers(
            Some("https://vote.example"),
            Some("vote.example")
        )));
        assert!(same_origin(&headers(
            Some("http://127.0.0.1:3000"),
            Some("127.0.0.1:3000")
        )));
        assert!(!same_origin(&headers(
            Some("https://evil.example"),
            Some("vote.example")
        )));
        assert!(!same_origin(&headers(
            Some("https://vote.example.evil.example"),
            Some("vote.example")
        )));
        assert!(!same_origin(&headers(None, Some("vote.example"))));
        assert!(!same_origin(&headers(Some("https://vote.example"), None)));
    }
}
