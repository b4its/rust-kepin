use mongodb::{Client, Database};
use crate::repository::user_repo::UserRepository;

pub struct AppState {
    pub db: Database,
    pub user_repo: UserRepository,
    pub kolosal_key: String,
}

pub async fn init_db() -> Database {
    let uri = std::env::var("MONGODB_URI").unwrap();
    let client = Client::with_uri_str(uri).await.unwrap();
    client.database("kepin") // Pastikan nama DB sesuai
}