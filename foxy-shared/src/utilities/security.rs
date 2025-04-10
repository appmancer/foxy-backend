use rand::{Rng, thread_rng};
use rand::distributions::Alphanumeric;


/// Generates a secure password with Cognito-compliant complexity
pub fn generate_secure_password() -> String {
    let mut rng = thread_rng();

    let password: String = (0..12) // Generate 12 characters
        .map(|_| rng.sample(Alphanumeric) as char) // Uses `sample()`
        .collect();

    format!("{password}@1") // Ensure Cognito complexity (symbol & digit)
}

