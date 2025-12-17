use axum::{
    extract::Multipart,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tokio::fs::{File, create_dir_all};
use tokio::io::AsyncWriteExt;
use std::path::Path;
use chrono::Local;

pub async fn upload_file(mut multipart: Multipart) -> impl IntoResponse {
    let mut user_id = String::from("unknown");
    let mut file_data = Vec::new();
    let mut file_name = String::new();
    let mut content_type_folder = String::from("others");

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "user_id" => {
                user_id = field.text().await.unwrap_or_else(|_| "unknown".into());
            }
            "file" => {
                let mime = field.content_type().unwrap_or("application/octet-stream").to_string();
                
                // Menentukan folder berdasarkan tipe mime
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

    // --- PERBAIKAN DI SINI: Mengambil Ekstensi Asli ---
    let extension = Path::new(&file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or(""); // Kosong jika tidak ada ekstensi

    let dt = Local::now().format("%Y%m%d_%H%M%S").to_string();
    
    // Gabungkan timestamp dengan ekstensi (jika ada)
    let safe_name = if extension.is_empty() {
        format!("{}_{}", dt, content_type_folder)
    } else {
        format!("{}_{}.{}", dt, content_type_folder, extension)
    };
    // ------------------------------------------------

    let upload_path = format!("media/{}/{}", user_id, content_type_folder);
    
    if let Err(e) = create_dir_all(&upload_path).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Dir Error: {}", e)).into_response();
    }

    let full_path = Path::new(&upload_path).join(&safe_name);

    let mut file = match File::create(&full_path).await {
        Ok(f) => f,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("File Error: {}", e)).into_response(),
    };

    if file.write_all(&file_data).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Write Error").into_response();
    }

    (StatusCode::OK, Json(json!({
        "status": "success",
        "file_original": file_name,
        "saved_as": safe_name,
        "path": full_path.to_str(),
        // URL yang bisa diakses via browser
        "url": format!("/public/{}/{}/{}", user_id, content_type_folder, safe_name)
    }))).into_response()
}