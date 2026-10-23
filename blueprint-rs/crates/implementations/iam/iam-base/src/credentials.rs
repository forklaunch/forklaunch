//! Shared password storage primitives for IAM implementations.
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;

/// Produces a PHC-encoded Argon2id hash using independent random salts.
///
/// # Errors
/// Returns a safe error on random source or hash failure.
pub fn hash_password(password: &str) -> BlueprintResult<String> {
    let mut salt = [0_u8; 16];
    getrandom::fill(&mut salt).map_err(|_| credential_error())?;
    Argon2::default()
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| credential_error())
}

/// Verifies a stored PHC hash using its encoded parameters. Malformed hashes fail closed.
#[must_use]
pub fn verify_password(password: &str, encoded_hash: &str) -> bool {
    PasswordHash::new(encoded_hash).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

fn credential_error() -> BlueprintError {
    BlueprintError::new("password_hash_failed", "Unable to securely hash password")
}

#[cfg(test)]
mod tests;
