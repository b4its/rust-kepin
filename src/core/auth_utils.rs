// src/core/auth_utils.rs
pub fn create_token(user_id: &str) -> String {
    // Implementasi JWT di sini
    format!("token_for_{}", user_id)
}