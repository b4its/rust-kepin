use axum::{
    extract::State, 
    http::StatusCode, 
    response::IntoResponse, 
    Json
};
use std::sync::Arc;
use bcrypt::{hash, verify, DEFAULT_COST};
use tower_cookies::{Cookies, Cookie};
use crate::db::AppState;
use crate::models::user::{User, AuthRequest};
use serde_json::json;

pub async fn register(
    State(state): State<Arc<AppState>>, 
    Json(payload): Json<AuthRequest>
) -> impl IntoResponse {
    // 1. Cek User Exist
    if state.user_repo.find_by_email(&payload.email).await.is_some() {
        return (StatusCode::CONFLICT, "Email already exists").into_response();
    }

    // 2. Hash Password (CPU-bound)
    let hashed_password = tokio::task::spawn_blocking(move || {
        hash(payload.password, DEFAULT_COST).unwrap()
    }).await.unwrap();

    let new_user = User {
        id: None,
        email: payload.email,
        name: payload.name.unwrap_or_default(),
        password: hashed_password,
        plan: "basic".to_string(),
    };

    // 3. Save
    match state.user_repo.create_user(new_user).await {
        Ok(_) => (StatusCode::CREATED, "Register Successfuly").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database Error").into_response(),
    }
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(payload): Json<AuthRequest>,
) -> impl IntoResponse {
    let user = match state.user_repo.find_by_email(&payload.email).await {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response(),
    };

    let is_valid = tokio::task::spawn_blocking(move || {
        verify(payload.password, &user.password).unwrap_or(false)
    }).await.unwrap();

    if !is_valid {
        return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response();
    }

    // Gunakan user.email sebagai value cookie
    let mut cookie = Cookie::new("session", user.email.clone()); 
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    cookies.add(cookie);

    (StatusCode::OK, "Login successful").into_response()
}

pub async fn logout(cookies: Cookies) -> impl IntoResponse {
    let mut cookie = Cookie::new("session", "");
    cookie.set_path("/");
    cookies.remove(cookie);
    (StatusCode::OK, "Logged out").into_response()
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
) -> impl IntoResponse {
    if let Some(cookie) = cookies.get("session") {
        let email = cookie.value();
        if let Some(user) = state.user_repo.find_by_email(email).await {
            return (StatusCode::OK, Json(json!({
                "name": user.name,
                "email": user.email,
                "avatar": "" // Anda bisa menambahkan field avatar di DB nanti
            }))).into_response();
        }
    }
    
    (StatusCode::UNAUTHORIZED, "Not logged in").into_response()
}