//! One-time token that gates first-run account creation.
//!
//! Without it, whoever reaches a fresh install first becomes the admin. The
//! token is printed on the server's stderr only (never through `tracing`), so
//! setup needs access to the operator's console. It lives in memory and dies with the
//! first account.

use std::sync::Mutex;

use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

/// Crockford base32: no `I`, `L`, `O`, `U`, so a token read off a log is hard to mistype.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const GROUP_LEN: usize = 5;

/// A fresh token, `XXXXX-XXXXX` (50 bits).
///
/// Short on purpose: it is typed by hand, guesses are online only and
/// rate-limited per IP, and it dies with the first account.
pub fn generate() -> String {
    let mut bytes = [0u8; GROUP_LEN * 2];
    OsRng.fill_bytes(&mut bytes);
    // 32 symbols: the low 5 bits index the alphabet with no modulo bias.
    let symbols: Vec<char> = bytes
        .iter()
        .map(|byte| ALPHABET[usize::from(byte & 0x1f)] as char)
        .collect();
    let (first, second) = symbols.split_at(GROUP_LEN);
    format!(
        "{}-{}",
        first.iter().collect::<String>(),
        second.iter().collect::<String>()
    )
}

/// The dash is only for readability, so dashes, whitespace and case never matter.
fn normalize(token: &str) -> String {
    token
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}

/// Comparing digests keeps the compared length fixed, so neither the token's
/// length nor its matching prefix leaks through timing.
fn digests_match(expected: &str, submitted: &str) -> bool {
    let expected = Sha256::digest(normalize(expected).as_bytes());
    let submitted = Sha256::digest(normalize(submitted).as_bytes());
    expected
        .iter()
        .zip(submitted.iter())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

/// The token for this process, or nothing once setup is done.
#[derive(Debug, Default)]
pub struct SetupToken {
    current: Mutex<Option<String>>,
}

impl SetupToken {
    /// Issues a token only while the install has no users.
    pub fn for_user_count(user_count: i64) -> Self {
        if user_count > 0 {
            return Self::default();
        }
        Self::with_value(&generate())
    }

    /// A fixed token, so tests do not depend on the random one.
    pub fn with_value(token: &str) -> Self {
        Self {
            current: Mutex::new(Some(token.to_string())),
        }
    }

    /// The token to show the operator, if one is active.
    pub fn current(&self) -> Option<String> {
        self.lock().clone()
    }

    /// `false` when no token is active, so a finished install never matches.
    pub fn verify(&self, submitted: &str) -> bool {
        self.lock()
            .as_deref()
            .is_some_and(|expected| digests_match(expected, submitted))
    }

    pub fn clear(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        // The guarded value is a plain Option; a poisoned lock holds no broken invariant.
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_token_has_two_groups_from_the_alphabet() {
        let token = generate();
        let (first, second) = token.split_once('-').expect("dash separator");
        for group in [first, second] {
            assert_eq!(group.len(), GROUP_LEN);
            assert!(group.bytes().all(|b| ALPHABET.contains(&b)), "got {token}");
        }
    }

    #[test]
    fn generated_tokens_differ() {
        assert_ne!(generate(), generate());
    }

    #[test]
    fn verify_accepts_the_exact_token() {
        assert!(SetupToken::with_value("ABCDE-FGHJK").verify("ABCDE-FGHJK"));
    }

    #[test]
    fn verify_ignores_dash_case_and_whitespace() {
        let token = SetupToken::with_value("ABCDE-FGHJK");
        assert!(token.verify("abcdefghjk"));
        assert!(token.verify("  abcde - fghjk \n"));
    }

    #[test]
    fn verify_rejects_wrong_empty_and_truncated_tokens() {
        let token = SetupToken::with_value("ABCDE-FGHJK");
        assert!(!token.verify("ABCDE-FGHJM"));
        assert!(!token.verify(""));
        assert!(!token.verify("ABCDE"));
    }

    #[test]
    fn no_token_is_issued_when_users_exist() {
        let token = SetupToken::for_user_count(1);
        assert_eq!(token.current(), None);
        assert!(!token.verify(""));
        assert!(!token.verify("ABCDE-FGHJK"));
    }

    #[test]
    fn token_is_issued_on_an_empty_install() {
        assert!(SetupToken::for_user_count(0).current().is_some());
    }

    #[test]
    fn clear_invalidates_the_token() {
        let token = SetupToken::with_value("ABCDE-FGHJK");
        token.clear();
        assert!(!token.verify("ABCDE-FGHJK"));
    }
}
