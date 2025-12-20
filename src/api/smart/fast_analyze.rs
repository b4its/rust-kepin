// Hapus baris ax_log
use axum::{
    extract::{Json, State},
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse},
};
use std::{convert::Infallible, time::Duration, sync::Arc, path::Path};
use tokio::fs;
use crate::db::AppState;
use crate::models::financial::{FinancialData, FinancialRecord, FinancialItem};
use chrono::Utc;
use crate::services::extractor_client::financial_proto::analyze_response::Result as ProtoResult; 

#[derive(serde::Deserialize)]
pub struct AnalyzeRequestDTO {
    pub file_path: String,
    pub user_id: String,
    pub id_userupload: String,
    #[serde(default = "default_mode")] 
    pub mode: String, 
}

fn default_mode() -> String {
    "normal".to_string()
}

pub async fn fast_analyze_document_stream(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AnalyzeRequestDTO>, 
) -> impl IntoResponse {
    // Audit ID unik untuk tracking satu sesi request
    let audit_id = format!("{}-{}", payload.user_id, Utc::now().timestamp_micros());
    
    // LOG AUDIT: Request Masuk
    println!("[AUDIT][{}] === NEW REQUEST ===", audit_id);
    println!("[AUDIT][{}] User: {}, File: {}, Mode: {}", audit_id, payload.user_id, payload.file_path, payload.mode);

    // 1. Validasi File
    let relative_path = payload.file_path.trim_start_matches("/public/");
    let file_path = Path::new("media").join(relative_path);
    
    if !file_path.exists() {
        println!("[AUDIT][{}] ERROR: File not found at {:?}", audit_id, file_path);
        return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().event("error").data("ERR_FILE: File not found"))
        ])).into_response();
    }

    // 2. Baca File
    let file_bytes = match fs::read(&file_path).await {
        Ok(b) => {
            println!("[AUDIT][{}] Success: File read ({} bytes)", audit_id, b.len());
            b
        },
        Err(e) => {
            println!("[AUDIT][{}] ERROR: Failed to read file: {}", audit_id, e);
            return Sse::new(futures::stream::iter(vec![
                Ok::<Event, Infallible>(Event::default().event("error").data(format!("ERR_READ: {}", e)))
            ])).into_response();
        }
    };

    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    let grpc_client = state.grpc_client.clone();
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
    let user_id = payload.user_id.clone();
    let upload_id = payload.id_userupload.clone();
    let state_clone = state.clone();
    let source_file_str = payload.file_path.clone();
    let analyze_mode = payload.mode.clone(); 

    // 3. Eksekusi Stream
    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().data(format!("INIT: [AuditID: {}] Memulai mode {}...", audit_id, analyze_mode)));

        println!("[AUDIT][{}] Calling gRPC Python Extractor...", audit_id);
        
        match grpc_client.analyze_stream(file_bytes, extension, filename, analyze_mode).await {
            Ok(mut grpc_stream) => {
                println!("[AUDIT][{}] gRPC Connection established.", audit_id);
                
                while let Ok(Some(msg)) = grpc_stream.message().await {
                    match msg.result {
                        Some(ProtoResult::LogMessage(log)) => {
                            // Kirim log AI ke frontend AND audit console
                            println!("[AUDIT][{}] gRPC Step: {}", audit_id, log);
                            yield Ok::<Event, Infallible>(Event::default().data(log));
                        },
                        Some(ProtoResult::ErrorMessage(err)) => {
                            println!("[AUDIT][{}] gRPC Logic Error: {}", audit_id, err);
                            yield Ok::<Event, Infallible>(Event::default().event("error").data(err));
                        },
                        Some(ProtoResult::FinalData(res)) => {
                            println!("[AUDIT][{}] Data received for entitas: {}", audit_id, res.nama_entitas);
                            
                            let data_lain: Vec<FinancialItem> = serde_json::from_str(&res.json_data_lain)
                                .unwrap_or_else(|_| {
                                    println!("[AUDIT][{}] WARN: Could not parse json_data_lain", audit_id);
                                    vec![]
                                });
                            
                            let record = FinancialRecord {
                                id: None,
                                user_id: user_id.clone(),
                                id_userupload: upload_id.clone(),
                                source_file: source_file_str.clone(),
                                data: FinancialData {
                                    nama_entitas: res.nama_entitas,
                                    periode_laporan: res.periode_laporan,
                                    mata_uang: res.mata_uang,
                                    satuan_angka: res.satuan_angka,
                                    total_aset: res.total_aset,
                                    total_liabilitas: res.total_liabilitas,
                                    total_ekuitas: res.total_ekuitas,
                                    laba_bersih: res.laba_bersih,
                                    data_keuangan_lain: data_lain,
                                },
                                created_at: Utc::now(),
                            };

                            println!("[AUDIT][{}] Saving to Database...", audit_id);
                            if let Err(e) = state_clone.financial_repo.save(record.clone()).await {
                                println!("[AUDIT][{}] DB ERROR: {}", audit_id, e);
                                yield Ok::<Event, Infallible>(Event::default().event("error").data(format!("DB_ERR: {}", e)));
                            } else {
                                println!("[AUDIT][{}] SUCCESS: Analysis saved to MongoDB", audit_id);
                                let json_str = serde_json::to_string(&record).unwrap_or_default();
                                yield Ok::<Event, Infallible>(Event::default().event("final_result").data(json_str));
                                yield Ok::<Event, Infallible>(Event::default().event("status").data("SAVED_DB"));
                            }
                        },
                        _ => {}
                    }
                }
            },
            Err(e) => {
                println!("[AUDIT][{}] gRPC Connection Failed: {}", audit_id, e);
                yield Ok::<Event, Infallible>(Event::default().event("error").data(format!("GRPC_CONN_ERR: {}", e)));
            }
        }
        println!("[AUDIT][{}] === END REQUEST ===\n", audit_id);
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(5)))
        .into_response()
}