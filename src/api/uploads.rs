use axum::{
    extract::{Multipart, State, Path, Query}, // Path di sini adalah axum::extract::Path
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tokio::fs::{File, create_dir_all, remove_file};
use tokio::io::AsyncWriteExt;
use std::path::{Path as StdPath}; // Kita rename Path standar jadi StdPath biar gak bentrok
use std::sync::Arc;
use chrono::{Local, Utc};

use crate::db::AppState;
use crate::models::upload::UserUpload;

// --- 1. Endpoint Upload File ---
pub async fn upload_file(
    State(state): State<Arc<AppState>>, 
    mut multipart: Multipart
) -> impl IntoResponse {
    
    let mut user_id = String::from("unknown");
    let mut file_data = Vec::new();
    let mut file_name = String::new();
    let mut content_type_folder = String::from("others");

    // Parsing Multipart
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "user_id" => {
                user_id = field.text().await.unwrap_or_else(|_| "unknown".into());
            }
            "file" => {
                let mime = field.content_type().unwrap_or("application/octet-stream").to_string();
                
                content_type_folder = if mime.starts_with("image/") { "images".into() } 
                                      else if mime.contains("pdf") { "documents".into() } 
                                      else { "others".into() };

                file_name = field.file_name().unwrap_or("unnamed").to_string();
                file_data = field.bytes().await.unwrap_or_default().to_vec();
            }
            _ => {}
        }
    }

    if file_data.is_empty() {
        return (StatusCode::BAD_REQUEST, "No file provided").into_response();
    }

    // Simpan File Fisik
    // Gunakan StdPath untuk manipulasi path file sistem
    let extension = StdPath::new(&file_name).extension().and_then(|ext| ext.to_str()).unwrap_or(""); 
    let dt = Local::now().format("%Y%m%d_%H%M%S").to_string();
    
    let safe_name = if extension.is_empty() {
        format!("{}_{}", dt, content_type_folder)
    } else {
        format!("{}_{}.{}", dt, content_type_folder, extension)
    };

    let upload_path = format!("media/{}/{}", user_id, content_type_folder);
    
    if let Err(e) = create_dir_all(&upload_path).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Dir Error: {}", e)).into_response();
    }

    let full_path = StdPath::new(&upload_path).join(&safe_name);
    let mut file = match File::create(&full_path).await {
        Ok(f) => f,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("File Error: {}", e)).into_response(),
    };

    if file.write_all(&file_data).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Write Error").into_response();
    }

    // URL yang bisa diakses Frontend
    let public_url = format!("/public/{}/{}/{}", user_id, content_type_folder, safe_name);

    // Simpan Metadata ke MongoDB
    let new_upload = UserUpload {
        id: None,
        user_id: user_id.clone(),
        file_name: safe_name.clone(),
        file_path: public_url.clone(),
        file_type: content_type_folder.clone(),
        created_at: Utc::now(),
    };

    if let Err(e) = state.upload_repo.create_upload(new_upload).await {
        eprintln!("Database Error: {}", e);
        // Tetap return OK karena file fisik tersimpan
    }

    (StatusCode::OK, Json(json!({
        "status": "success",
        "saved_as": safe_name,
        "url": public_url,
        "type": content_type_folder
    }))).into_response()
}

// --- 2. Endpoint Get My Uploads ---

#[derive(Deserialize)]
pub struct UserQuery {
    pub user_id: String,
}

pub async fn get_my_uploads(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UserQuery>
) -> impl IntoResponse {
    match state.upload_repo.find_by_user(&query.user_id).await {
        Ok(uploads) => (StatusCode::OK, Json(uploads)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response(),
    }
}

// --- 3. Endpoint Delete File ---
pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    
    // 1. Cari data upload di database (untuk mendapatkan path file fisik)
    let upload_record = match state.upload_repo.find_by_id(&id).await {
        Ok(Some(record)) => record,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error": "File not found"}))).into_response(),
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid ID format"}))).into_response(),
    };

    // 2. HAPUS OTOMATIS: Record di financial_reports
    // Karena id di sini adalah ID dari user_upload, kita gunakan sebagai filter
    if let Err(e) = state.financial_repo.delete_by_upload_id(&id).await {
        // Kita log errornya, tapi tetap lanjut menghapus file fisik
        eprintln!("Error deleting financial records for upload {}: {}", id, e);
    }

    // 3. Hapus File Fisik
    let relative_path = upload_record.file_path.trim_start_matches("/public/");
    let system_path = StdPath::new("media").join(relative_path);

    if let Err(e) = remove_file(&system_path).await {
        eprintln!("Warning: File fisik tidak ditemukan atau gagal dihapus: {}", e);
    }

    // 4. Hapus Record Upload (Induk)
    match state.upload_repo.delete(&id).await {
        Ok(count) => {
            if count > 0 {
                (StatusCode::OK, Json(json!({
                    "status": "success",
                    "message": "File, financial reports, and upload record deleted",
                    "id": id
                }))).into_response()
            } else {
                (StatusCode::NOT_FOUND, Json(json!({"error": "Upload record already gone"}))).into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}
