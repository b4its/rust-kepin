mod api;
mod core;
mod db;
mod models;
mod repository;

use axum::{
    routing::{post, get, delete}, // Tambahkan 'delete'
    Router,
    http::{header::{CONTENT_TYPE, AUTHORIZATION, COOKIE}, Method, HeaderValue}
};
use std::{sync::Arc, env, net::SocketAddr};
use crate::db::AppState;
use crate::repository::{user_repo::UserRepository, upload_repo::UploadRepository}; 
use tower_cookies::CookieManagerLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    
    // 1. Init Database
    let database = db::init_db().await;
    
    // 2. Setup State
    let state = Arc::new(AppState {
        db: database.clone(),
        user_repo: UserRepository::new(&database),
        upload_repo: UploadRepository::new(&database), 
        kolosal_key: env::var("KOLOSAL_API_KEY").unwrap_or_else(|_| "default".to_string()),
    });

    // 3. CORS Setup
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:3000".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE]) // Pastikan DELETE diizinkan
        .allow_headers([CONTENT_TYPE, AUTHORIZATION, COOKIE])
        .allow_credentials(true);

    // 4. Router Setup
    let app = Router::new()
        .nest_service("/public", ServeDir::new("media"))
        
        .nest("/api/v1", Router::new()
            .nest("/auth", Router::new()
                .route("/register", post(api::auth::register))
                .route("/login", post(api::auth::login))
                .route("/logout", post(api::auth::logout))
                .route("/me", get(api::auth::me))
            )
            // Upload Routes
            .route("/upload", post(api::uploads::upload_file))        // Create
            .route("/upload/:id", delete(api::uploads::delete_file))  // DELETE (BARU)
            .route("/uploads", get(api::uploads::get_my_uploads))     // Read List
            .route("/analyze", post(api::analyze::analyze_document_stream))
        )
        .layer(cors)
        .layer(CookieManagerLayer::new())
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "8000".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().expect("Invalid address");
    
    println!("🚀 Server running on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}