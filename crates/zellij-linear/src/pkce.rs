//! RFC 7636 PKCE helpers and a small CSRF state generator.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// 32 random bytes → 43-char base64url verifier (within RFC range 43..=128).
pub fn generate_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// S256 challenge: base64url(sha256(verifier_ascii)).
pub fn challenge_from_verifier(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// 24-byte random base64url string used as the OAuth `state` value to
/// catch CSRF.
pub fn generate_state() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7636 §A.1 test vector.
    #[test]
    fn rfc7636_a1_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(challenge_from_verifier(verifier), expected);
    }

    #[test]
    fn verifier_is_43_chars_base64url() {
        let v = generate_verifier();
        assert_eq!(v.len(), 43);
        assert!(v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn state_is_32_chars_base64url() {
        // 24 bytes → 32 chars without padding.
        let s = generate_state();
        assert_eq!(s.len(), 32);
    }

    #[test]
    fn distinct_calls_produce_distinct_values() {
        assert_ne!(generate_verifier(), generate_verifier());
        assert_ne!(generate_state(), generate_state());
    }
}
