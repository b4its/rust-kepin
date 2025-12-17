use ax_compression::CompressionLayer; // Tambahkan jika ingin kompresi
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    // 1. Load env dengan manajemen error yang lebih baik
    dotenvy::dotenv().ok();
    
    // 2. Inisialisasi DB (Pastikan connection pool sudah dioptimalkan di db::init_db)
    let database = db::init_db().await;
    
    // 3. State menggunakan Arc untuk sharing yang efisien antar thread
    let state = Arc::new(AppState {
        db: database.clone(),
        user_repo: repository::user_repo::UserRepository::new(&database),
        kolosal_key: env::var("KOLOSAL_API_KEY").expect("KOLOSAL_API_KEY missing"),
    });

    // 4. Bangun Router secara efisien (In-place nesting)
    let app = Router::new()
        .nest("/api/v1", Router::new()
            .nest("/auth", Router::new()
                .route("/register", post(api::auth::register))
                .route("/login", post(api::auth::login))
                .route("/logout", post(api::auth::logout))
            )
        )
        // Layer diletakkan di luar nest agar dieksekusi sekali untuk semua rute
        .layer(CookieManagerLayer::new())
        .with_state(state);

    // 5. Optimasi Listener & Networking
    let port = env::var("PORT").unwrap_or_else(|_| "8000".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().expect("Invalid address");
    
    let listener = tokio::net::TcpListener::bind(addr).await.expect("Failed to bind");
    
    // Set TCP Keepalive atau buffer jika diperlukan untuk traffic tinggi
    println!("🚀 High Performance Server running on {}", addr);
    
    // 6. Jalankan server dengan Axum Serve
    axum::serve(listener, app)
        // .with_graceful_shutdown(shutdown_signal()) // Opsional: Untuk keamanan data saat restart
        .await
        .expect("Server error");
}