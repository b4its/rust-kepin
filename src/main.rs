// 1. Deklarasi Module (Sangat Penting!)
mod api;
mod core;
mod db;
mod models;
mod repository;

// 2. Import Library
use axum::{routing::post, Router};
use std::{sync::Arc, env, net::SocketAddr};
use crate::db::AppState; // Pastikan AppState ada di src/db/mod.rs atau src/db.rs
use tower_cookies::CookieManagerLayer;

#[tokio::main]
async fn main() {
    // Load env
    dotenvy::dotenv().ok();
    
    // Inisialisasi DB
    let database = db::init_db().await;
    
    // State
    let state = Arc::new(AppState {
        db: database.clone(),
        user_repo: repository::user_repo::UserRepository::new(&database),
        kolosal_key: env::var("KOLOSAL_API_KEY").expect("KOLOSAL_API_KEY missing"),
    });

    // 3. Routing dengan Nesting Berlapis
    let app = Router::new()
        .nest("/api/v1", Router::new()
            .nest("/auth", Router::new()
                .route("/register", post(api::auth::register))
                .route("/login", post(api::auth::login))
                .route("/logout", post(api::auth::logout))
            )
        )
        .layer(CookieManagerLayer::new()) // WAJIB ADA
        .with_state(state);

    // 4. Konfigurasi Port & Address
    let port = env::var("PORT").unwrap_or_else(|_| "8000".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port)
        .parse()
        .expect("Invalid address format");
    
    println!("🚀 Server running on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}