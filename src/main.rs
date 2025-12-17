mod api;
mod core;
mod db;
mod models;
mod repository;

use axum::{
    routing::{post, get}, 
    Router,
    http::{header::{CONTENT_TYPE, AUTHORIZATION, COOKIE}, Method, HeaderValue}
};
use std::{sync::Arc, env, net::SocketAddr};
use crate::db::AppState;
use tower_cookies::CookieManagerLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    
    let database = db::init_db().await;
    
    let state = Arc::new(AppState {
        db: database.clone(),
        user_repo: repository::user_repo::UserRepository::new(&database),
        kolosal_key: env::var("KOLOSAL_API_KEY").expect("KOLOSAL_API_KEY missing"),
    });

    let cors = CorsLayer::new()
        .allow_origin("http://localhost:3000".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION, COOKIE])
        .allow_credentials(true);

    let app = Router::new()
        .nest_service("/public", ServeDir::new("media"))
        .nest("/api/v1", Router::new()
            .nest("/auth", Router::new()
                .route("/register", post(api::auth::register))
                .route("/login", post(api::auth::login))
                .route("/logout", post(api::auth::logout))
                .route("/me", get(api::auth::me))
            )
            // Endpoint Upload
            .route("/upload", post(api::uploads::upload_file))
        )
        // Serve file statis dari folder "uploads" di path "/public"
        .layer(cors)
        .layer(CookieManagerLayer::new())
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "8000".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().expect("Invalid address");
    
    println!("🚀 Server running on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}