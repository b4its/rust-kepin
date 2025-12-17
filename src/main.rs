mod api;
mod core;
mod db;
mod models;
mod repository;

use axum::{routing::post, Router};
use std::{sync::Arc, env};
use crate::db::AppState;
use tower_cookies::CookieManagerLayer;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    
    // Inisialisasi DB sekali saja 
    let database = db::init_db().await;
    
    // Masukkan Repository ke dalam State agar bisa diakses API
    let state = Arc::new(AppState {
        db: database.clone(),
        user_repo: repository::user_repo::UserRepository::new(&database),
        kolosal_key: env::var("KOLOSAL_API_KEY").expect("KOLOSAL_API_KEY missing"),
    });

    let app = Router::new()
        .nest("/api/auth", Router::new()
            .route("/register", post(api::auth::register))
            .route("/login", post(api::auth::login))
            .route("/logout", post(api::auth::logout))
        )
        .layer(CookieManagerLayer::new()) // WAJIB ADA
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "8000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    
    println!("🚀 Server running on port {}", port);
    axum::serve(listener, app).await.unwrap();
}