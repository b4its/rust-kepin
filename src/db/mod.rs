use mongodb::{Client, Database};
use crate::repository::user_repo::UserRepository;
use crate::repository::upload_repo::UploadRepository;
use crate::repository::financial_repo::FinancialRepository; // Import baru

pub struct AppState {
    pub db: mongodb::Database,
    pub user_repo: crate::repository::user_repo::UserRepository,
    pub upload_repo: crate::repository::upload_repo::UploadRepository,
    pub financial_repo: FinancialRepository, // Tambah field ini
    pub kolosal_key: String,
}

pub async fn init_db() -> Database {
    let uri = std::env::var("MONGODB_URI").expect("MONGODB_URI error");
    let client = Client::with_uri_str(uri).await.unwrap();
    client.database("kepin")
}