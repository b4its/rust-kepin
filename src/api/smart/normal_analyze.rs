use axum::{
    extract::{Json, State, Query}, 
    http::StatusCode,
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse},
};
use futures::stream::StreamExt;
use reqwest::{header::{AUTHORIZATION, CONTENT_TYPE}, Client};
use serde::Deserialize;
use serde_json::json;
use std::{
    convert::Infallible,
    fmt::Write,
    io::Cursor,
    path::Path,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{fs, task};
use calamine::{Data, Reader, Xlsx};
use chrono::Utc;

use crate::db::AppState;
use crate::models::financial::{FinancialData, FinancialRecord};

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

// --- DTO: Request Body untuk Analisa ---
#[derive(Deserialize)]
pub struct AnalyzeRequest {
    pub file_path: String,
    pub user_id: String,
    pub id_userupload: String, // Wajib dikirim frontend
}

// --- DTO: Query Param untuk GET Data ---
#[derive(Deserialize)]
pub struct FinancialQuery {
    pub user_id: String,
}

// --- Helper Structs Parsing AI ---
#[derive(Deserialize, Debug)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}
#[derive(Deserialize, Debug)]
struct StreamChoice {
    delta: StreamDelta,
}
#[derive(Deserialize, Debug)]
struct StreamDelta {
    content: Option<String>,
}

// --- Handler 1: GET Financial Data ---
pub async fn get_financial_data(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FinancialQuery>
) -> impl IntoResponse {
    match state.financial_repo.find_by_user(&query.user_id).await {
        Ok(records) => (StatusCode::OK, Json(records)).into_response(),
        Err(e) => {
            eprintln!("Database Error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed fetch"}))).into_response()
        }
    }
}


// dashboard get financial stats
pub async fn get_financial_stats(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FinancialQuery>
) -> impl IntoResponse {
    // Panggil fungsi yang ada di dalam repository melalui state
    match state.upload_repo.get_uploads_stats(&query.user_id).await {
        Ok(stats_data) => {
            (StatusCode::OK, Json(serde_json::json!({
                "status": "success",
                "data": stats_data
            }))).into_response()
        },
        Err(e) => {
            eprintln!("Dashboard Stats Error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR, 
                Json(serde_json::json!({ "status": "error", "message": "Gagal menghitung statistik" }))
            ).into_response()
        }
    }
}



// --- Helper: Excel Parser ---
fn normal_excel_bytes_to_csv_optimized(bytes: Vec<u8>, limit: usize) -> Result<String, String> {
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
                    if !first { buffer.push(','); }
                    first = false;
                    match c {
                        Data::String(s) => { buffer.push('"'); buffer.push_str(s); buffer.push('"'); },
                        Data::Float(f) => { let _ = write!(buffer, "{}", f); },
                        Data::Int(i) => { let _ = write!(buffer, "{}", i); },
                        Data::Bool(b) => { let _ = write!(buffer, "{}", b); },
                        Data::DateTime(d) => { let _ = write!(buffer, "{}", d); },
                        Data::DateTimeIso(d) => { let _ = write!(buffer, "{}", d); },
                        Data::DurationIso(d) => { let _ = write!(buffer, "{}", d); }, 
                        Data::Error(_) => buffer.push_str("ERR"),
                        Data::Empty => {}, 
                        _ => {}, // Catch all variant
                    }
                }
                buffer.push('\n');
            }
        }
    }
    if buffer.is_empty() { return Err("File Excel kosong/rusak".to_string()); }
    Ok(buffer)
}

// --- Handler 2: Analyze Stream (POST) ---
pub async fn normal_analyze_document_stream(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AnalyzeRequest>,
) -> impl IntoResponse {
    
    let relative_path = payload.file_path.trim_start_matches("/public/");
    let file_path = Path::new("media").join(relative_path);
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    
    let file_bytes = match fs::read(&file_path).await {
        Ok(b) => b,
        Err(e) => return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().data(format!("ERR_FILE: {}", e)))
        ])).into_response(),
    };

    const CHAR_LIMIT: usize = 50_000;
    let content_result = if ["xlsx", "xls"].contains(&extension.as_str()) {
        task::spawn_blocking(move || normal_excel_bytes_to_csv_optimized(file_bytes, CHAR_LIMIT)).await.unwrap_or(Err("Thread Error".to_string()))
    } else if ["csv", "txt", "json", "md", "html"].contains(&extension.as_str()) {
        match String::from_utf8(file_bytes) {
            Ok(mut s) => {
                if s.len() > CHAR_LIMIT { s.truncate(CHAR_LIMIT); s.push_str("..."); }
                Ok(s)
            },
            Err(_) => Err("Non-UTF8".into()),
        }
    } else {
        return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().data(format!("ERR_FMT: .{}", extension)))
        ])).into_response();
    };

    let truncated_content = match content_result {
        Ok(c) => c,
        Err(e) => return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().data(format!("ERR_PARSE: {}", e)))
        ])).into_response(),
    };

    let system_prompt = r#"You are a high-precision Financial Data Extraction Engine specialized in Indonesian financial statements (Laporan Keuangan). 
    Your goal is to parse MULTIPLE SHEETS and consolidate data into a single, strict JSON output.

    ### Extraction Rules:
    1.  **Priority & Period**: Only extract data for the "Current Period" (Periode Berjalan). Explicitly ignore columns labeled "Prior Year", "Comparative", or "Audit Sebelumnya".
    2.  **Numeric Integrity**: 
        - Extract raw numbers only. Do not perform any arithmetic.
        - Format: Convert (1,234.56) or "1.234,56-" into a standard negative number: -1234.56.
        - If a value is dash "-" or "nil", treat it as 0.
    3.  **Smart Matching**: Use fuzzy matching for Indonesian/English financial terms.
        - `total_aset`: (Total Assets)
        - `total_liabilitas`: (Total Liabilities)
        - `total_ekuitas`: (Total Equity)
        - `laba_bersih`: (Net Profit/Loss, Laba Tahun Berjalan, Profit attributable to owners)
    4.  **Metadata**: 
        - `nama_entitas`: Find the legal entity name on the cover or header.
        - `periode_laporan`: Convert to ISO-8601 (YYYY-MM-DD) based on the balance sheet date.
        - `satuan_angka`: Detect if numbers are in Full, Thousands (Ribuan), or Millions (Jutaan).

    ### Data Keuangan Lain (Contextual Extraction):
    Extract 5-10++ additional significant line items (e.g., Pendapatan/Revenue, Beban Pokok/COGS, Kas/Cash) that characterize the company's performance.

    ### Output Schema (Strict JSON):
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

    let user_prompt = format!("ANALYZE DATA:\n---\n{}\n---\nOutput JSON only.", truncated_content);

    let client = HTTP_CLIENT.get_or_init(|| Client::builder().build().unwrap_or_default());

    let req_builder = client.post("https://api.kolosal.ai/v1/chat/completions")
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", state.kolosal_key))
        .json(&json!({
            "model": "Kimi K2",
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": true,
            "temperature": 0.1,
            "response_format": { "type": "json_object" },
            "max_tokens": 3000
        }));

    let state_clone = state.clone();
    let file_path_str = payload.file_path.clone();
    let current_user_id = payload.user_id.clone();
    let current_id_userupload = payload.id_userupload.clone(); 

    let stream = async_stream::stream! {
        let response = match req_builder.send().await {
            Ok(r) => r,
            Err(e) => { yield Ok::<Event, Infallible>(Event::default().data(format!("ERR_CONN: {}", e))); return; }
        };

        if !response.status().is_success() {
             let err = response.text().await.unwrap_or_default();
             yield Ok::<Event, Infallible>(Event::default().data(format!("ERR_API: {}", err)));
             return;
        }

        let mut byte_stream = response.bytes_stream();
        let mut full_log = String::new();
        let mut json_str = String::new();

        while let Some(chunk) = byte_stream.next().await {
            if let Ok(bytes) = chunk {
                let text = String::from_utf8_lossy(&bytes);
                full_log.push_str(&text);
                
                for line in text.lines() {
                    if let Some(raw) = line.strip_prefix("data: ") {
                        if raw.trim() == "[DONE]" { continue; }
                        if let Ok(parsed) = serde_json::from_str::<StreamChunk>(raw) {
                            if let Some(c) = parsed.choices.first() {
                                if let Some(content) = &c.delta.content {
                                    json_str.push_str(content);
                                }
                            }
                        }
                    }
                }
                yield Ok::<Event, Infallible>(Event::default().data(text.to_string()));
            }
        }

        let clean_json = json_str.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
        println!("\n=== LOG: {} ===\n{}\n", file_path_str, clean_json);
        
        match serde_json::from_str::<FinancialData>(clean_json) {
            Ok(financial_data) => {
                let record = FinancialRecord {
                    id: None,
                    user_id: current_user_id.clone(),
                    id_userupload: current_id_userupload, // <--- DISIMPAN KE DB
                    source_file: file_path_str,
                    data: financial_data,
                    created_at: Utc::now(),
                };

                match state_clone.financial_repo.save(record.clone()).await {
                    Ok(_) => {
                        println!("✅ [DB] Saved.");
                        // KIRIM DATA LENGKAP YANG BARU DISIMPAN KE FRONTEND
                        let saved_json = serde_json::to_string(&record).unwrap_or_default();
                        yield Ok::<Event, Infallible>(
                            Event::default()
                                .event("final_result") // Nama event khusus
                                .data(saved_json)
                        );
                        
                        yield Ok::<Event, Infallible>(Event::default().event("status").data("SAVED_DB"));
                    },
                    Err(e) => {
                        eprintln!("❌ [DB] Error: {}", e);
                        yield Ok::<Event, Infallible>(Event::default().event("error").data(format!("DB_ERR: {}", e)));
                    }
                }
            },
            Err(_) => {
                eprintln!("❌ [JSON] Parse Error");
                yield Ok::<Event, Infallible>(Event::default().event("error").data("ERR_JSON_PARSE"));
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response()
}