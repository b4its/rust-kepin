mod api;
mod core;
mod db;
mod models;
mod repository;
mod services; // 1. Tambahkan ini agar folder services dikenali

use axum::{
    routing::{post, get, delete},
    Router,
    http::{header::{CONTENT_TYPE, AUTHORIZATION, COOKIE}, Method, HeaderValue},
};
use std::{sync::Arc, env, net::SocketAddr};

use crate::db::AppState;
use crate::repository::{user_repo::UserRepository, upload_repo::UploadRepository, financial_repo::FinancialRepository}; 
use crate::services::extractor_client::GrpcClient; // 2. Import GrpcClient
use tower_cookies::CookieManagerLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    
    let database = db::init_db().await;

    // 3. Inisialisasi gRPC Client (Pastikan Python service jalan di port ini)
    let grpc_addr = env::var("GRPC_EXTRACTOR_URL").unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());
    let grpc_client = GrpcClient::connect(grpc_addr)
        .await
        .expect("❌ Gagal terhubung ke Python gRPC Service");
    
    let state = Arc::new(AppState {
        db: database.clone(),
        user_repo: UserRepository::new(&database),
        upload_repo: UploadRepository::new(&database),
        financial_repo: FinancialRepository::new(&database),
        kolosal_key: env::var("KOLOSAL_API_KEY").unwrap_or_else(|_| "default".to_string()),
        grpc_client, // 4. Masukkan ke AppState
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
            .route("/upload", post(api::uploads::upload_file))
            .route("/upload/:id", delete(api::uploads::delete_file))
            .route("/uploads", get(api::uploads::get_my_uploads))
            .route("/normal_analyze", post(api::normal_analyze::normal_analyze_document_stream))
            .route("/fast_analyze", post(api::fast_analyze::fast_analyze_document_stream))
            .route("/deep_analyze", post(api::deep_analyze::deep_analyze_document_stream))
            .route("/financial-data", get(api::normal_analyze::get_financial_data))
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