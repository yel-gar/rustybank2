use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

pub fn hash_password(password: String) -> anyhow::Result<String> {
    let argon = Argon2::default();
    Ok(argon
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))?
        .to_string())
}

pub fn verify_password(password: String, hash: String) -> bool {
    let argon = Argon2::default();
    let hash = match PasswordHash::new(&hash) {
        Ok(hash) => hash,
        Err(_) => return false,
    };
    argon.verify_password(password.as_bytes(), &hash).is_ok()
}
