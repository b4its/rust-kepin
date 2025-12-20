use axum::{
    extract::{Json, State},
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse},
};
use futures::stream::StreamExt;
use reqwest::{header::{AUTHORIZATION, CONTENT_TYPE}, Client};
use serde::Deserialize; // Pastikan ini ada
use serde_json::json;
use std::{convert::Infallible, time::Duration, sync::{Arc, OnceLock}, path::Path};
use tokio::{fs, task};
use std::io::Cursor;
use calamine::{Reader, Xlsx, Data};
use chrono::Utc;
use std::fmt::Write; 

use crate::db::AppState;
use crate::models::financial::{FinancialData, FinancialRecord};
use crate::services::extractor_client::financial_proto::analyze_response::Result as ProtoResult; 

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

// --- STRUCT REQUEST (Pastikan ini ada di file ini) ---
#[derive(Deserialize)]
pub struct AnalyzeRequest {
    pub file_path: String,
    pub user_id: String,
    pub id_userupload: String,
}

// --- HELPER PARSING ---
fn deep_excel_bytes_to_csv_optimized(bytes: Vec<u8>, limit: usize) -> Result<String, String> {
    let cursor = Cursor::new(bytes);
    let mut workbook = Xlsx::new(cursor).map_err(|e| e.to_string())?;
    
    let mut buffer = String::with_capacity(limit + 1024); 
    let sheet_names = workbook.sheet_names().to_owned();

    'outer: for name in sheet_names {
        if let Ok(range) = workbook.worksheet_range(&name) {
            let _ = writeln!(buffer, "\n--- SHEET: {} ---", name);
            for row in range.rows() {
                if buffer.len() >= limit { break 'outer; }
                let mut first = true;
                for c in row.iter() {
                    if !first { buffer.push('|'); } 
                    first = false;
                    match c {
                        Data::String(s) => { buffer.push_str(s); },
                        Data::Float(f) => { let _ = write!(buffer, "{}", f); },
                        Data::Int(i) => { let _ = write!(buffer, "{}", i); },
                        Data::Bool(b) => { let _ = write!(buffer, "{}", b); },
                        Data::DateTime(d) => { let _ = write!(buffer, "{}", d); },
                        Data::DateTimeIso(d) => { let _ = write!(buffer, "{}", d); },
                        Data::DurationIso(d) => { let _ = write!(buffer, "{}", d); }, 
                        Data::Error(_) => buffer.push_str("ERR"),
                        Data::Empty => {}, 
                        // Baris catch-all dihapus untuk hilangkan warning
                    }
                }
                buffer.push('\n');
            }
        }
    }
    if buffer.is_empty() { return Err("File Excel kosong".to_string()); }
    Ok(buffer)
}

// --- HANDLER UTAMA ---
pub async fn deep_analyze_document_stream(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AnalyzeRequest>, 
) -> impl IntoResponse {

    let relative_path = payload.file_path.trim_start_matches("/public/");
    let file_path = Path::new("media").join(relative_path);
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let user_id = payload.user_id.clone();
    let upload_id = payload.id_userupload.clone();
    let file_path_str = payload.file_path.clone();
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();

    let file_bytes = match fs::read(&file_path).await {
        Ok(b) => b,
        Err(e) => return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().data(format!("ERR_FILE: {}", e)))
        ])).into_response(),
    };

    let grpc_client = state.grpc_client.clone();
    let state_clone = state.clone();

    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(Event::default().data("INIT: Memulai Deep Analysis dengan Hybrid Engine..."));

        yield Ok::<Event, Infallible>(Event::default().data("STEP 1: Generasi Konteks Teks (Local)..."));
        
        let file_bytes_clone = file_bytes.clone();
        let context_result = task::spawn_blocking(move || {
            deep_excel_bytes_to_csv_optimized(file_bytes_clone, 50_000)
        }).await.unwrap_or(Err("Thread error".into()));

        let raw_context = match context_result {
            Ok(s) => s,
            Err(e) => {
                yield Ok::<Event, Infallible>(Event::default().event("error").data(format!("ERR_PARSE: {}", e)));
                return;
            }
        };

        yield Ok::<Event, Infallible>(Event::default().data("STEP 2: Mengambil Data Heuristik (Python Engine)..."));
        
        let mut algo_guess_json = String::from("{}");
        
        // Panggil gRPC - Asumsi Anda punya method ini di client wrapper
        match grpc_client.analyze_stream(file_bytes.clone(), extension.clone(), filename, "normal".to_string()).await {
            Ok(mut grpc_stream) => {
                while let Ok(Some(msg)) = grpc_stream.message().await {
                    if let Some(ProtoResult::FinalData(res)) = msg.result {
                        let temp_json = json!({
                            "nama_entitas": res.nama_entitas,
                            "mata_uang": res.mata_uang,
                            "satuan_angka": res.satuan_angka,
                            "total_aset": res.total_aset,
                            "total_liabilitas": res.total_liabilitas,
                            "total_ekuitas": res.total_ekuitas,
                            "laba_bersih": res.laba_bersih
                        });
                        algo_guess_json = temp_json.to_string();
                    }
                }
            },
            Err(e) => {
                yield Ok::<Event, Infallible>(Event::default().data(format!("WARN: Gagal koneksi ke Engine Python. Error: {}", e)));
            }
        }

        yield Ok::<Event, Infallible>(Event::default().data("STEP 3: AI Agent Melakukan Validasi & Koreksi..."));

        let system_prompt = r#"You are a Lead Financial Auditor.
You have two inputs:
1. RAW EXCEL CONTENT: A pipe-separated CSV representation of the file.
2. ALGO GUESS: A JSON extracted by a rigid Regex algorithm.

YOUR MISSION:
1. **Validate Core Metrics**: Check Total Assets, Liabilities, Equity, and Net Income in 'ALGO GUESS' against 'RAW EXCEL CONTENT'. Fix any scaling errors (e.g., millions vs full amount).
2. **EXTRACT DETAILED 'data_keuangan_lain'**:
   - The 'ALGO GUESS' for this field is often incomplete.
   - You MUST scan the 'RAW EXCEL CONTENT' to find **10-20 key financial line items**.
   - Extract items such as:
     * Cash & Equivalents (Kas dan Setara Kas)
     * Trade Receivables (Piutang Usaha)
     * Inventories (Persediaan)
     * Fixed Assets (Aset Tetap)
     * Trade Payables (Utang Usaha)
     * Long-term Debt (Utang Jangka Panjang)
     * Revenue/Sales (Pendapatan/Penjualan)
     * Cost of Goods Sold (Beban Pokok)
     * Selling & Marketing Expenses (Beban Penjualan)
     * General & Admin Expenses (Beban Umum)
     * Finance Costs (Beban Keuangan)
     * Tax Expenses (Beban Pajak)
   - Use the original Indonesian or English names found in the doc for "keterangan".

OUTPUT SCHEMA (Strict JSON):
{
  "nama_entitas": "string",
  "periode_laporan": "YYYY-MM-DD",
  "mata_uang": "string",
  "satuan_angka": "string",
  "total_aset": number,
  "total_liabilitas": number,
  "total_ekuitas": number,
  "laba_bersih": number,
  "data_keuangan_lain": [ 
      { "keterangan": "string", "nilai": number }
  ]
}"#;

        let user_prompt = format!(
            "RAW EXCEL CONTENT:\n---\n{}\n---\n\nALGO GUESS (Validate Core, Expand Details):\n{}\n\nOutput Valid JSON Only.", 
            raw_context, 
            algo_guess_json
        );

        let client = HTTP_CLIENT.get_or_init(|| Client::builder().build().unwrap_or_default());
        let req = client.post("https://api.kolosal.ai/v1/chat/completions")
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", state_clone.kolosal_key))
            .json(&json!({
                "model": "Kimi K2", 
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ],
                "stream": true,
                "temperature": 0.1,
                "response_format": { "type": "json_object" }
            }));

        let mut ai_json_accumulated = String::new();

        match req.send().await {
            Ok(resp) => {
                let mut byte_stream = resp.bytes_stream();
                while let Some(chunk) = byte_stream.next().await {
                    if let Ok(bytes) = chunk {
                        let text = String::from_utf8_lossy(&bytes);
                        for line in text.lines() {
                            if let Some(raw) = line.strip_prefix("data: ") {
                                if raw.trim() == "[DONE]" { continue; }
                                if let Ok(parsed_val) = serde_json::from_str::<serde_json::Value>(raw) {
                                    if let Some(content) = parsed_val["choices"][0]["delta"]["content"].as_str() {
                                        ai_json_accumulated.push_str(content);
                                    }
                                }
                            }
                        }
                    }
                }
            },
            Err(e) => {
                 yield Ok::<Event, Infallible>(Event::default().event("error").data(format!("AI_CONN_ERR: {}", e)));
                 return;
            }
        }

        let clean_json = ai_json_accumulated.trim()
            .trim_start_matches("```json").trim_start_matches("```")
            .trim_end_matches("```").trim();

        match serde_json::from_str::<FinancialData>(clean_json) {
            Ok(financial_data) => {
                let record = FinancialRecord {
                    id: None,
                    user_id: user_id,
                    id_userupload: upload_id,
                    source_file: file_path_str,
                    data: financial_data,
                    created_at: Utc::now(),
                };

                match state_clone.financial_repo.save(record.clone()).await {
                    Ok(_) => {
                        let saved_json = serde_json::to_string(&record).unwrap_or_default();
                        yield Ok::<Event, Infallible>(Event::default().event("final_result").data(saved_json));
                        yield Ok::<Event, Infallible>(Event::default().event("status").data("SAVED_DB"));
                    },
                    Err(e) => yield Ok::<Event, Infallible>(Event::default().event("error").data(format!("DB_ERR: {}", e))),
                }
            },
            Err(_) => {
                yield Ok::<Event, Infallible>(Event::default().event("error").data("AI Failed to produce valid JSON"));
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response()
}