use mongodb::{Client, Database};
// Import User Repository (Existing)
use crate::repository::user_repo::UserRepository;
// Import Upload Repository (BARU - Tambahkan ini)
use crate::repository::upload_repo::UploadRepository;

pub struct AppState {
    pub db: Database,
    pub user_repo: UserRepository,
    pub upload_repo: UploadRepository, 
    pub kolosal_key: String,
}

pub async fn init_db() -> Database {
    // Pastikan variabel environment MONGODB_URI ada di .env
    let uri = std::env::var("MONGODB_URI").expect("MONGODB_URI tidak ditemukan di .env");
    
    let client = Client::with_uri_str(uri).await.unwrap();
    client.database("kepin") // Pastikan nama DB sesuai keinginan
}