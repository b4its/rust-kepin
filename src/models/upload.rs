// src/models/upload.rs
use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserUpload {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    
    pub user_id: String,      // idUser
    pub file_name: String,    // file (nama file asli/aman)
    pub file_path: String,    // file (path lengkap atau url)
    pub file_type: String,    // jenis file (images, documents, others)
    

    #[serde(with = "mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
}