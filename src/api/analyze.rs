use axum::{
    extract::{Json, State},
    response::sse::{Event, Sse, KeepAlive},
    response::IntoResponse,
};
use futures::stream::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, path::Path, time::Duration, convert::Infallible, io::Cursor};
use tokio::fs;
use tokio::task;
use calamine::{Reader, Xlsx, Data}; 
use crate::db::AppState;

#[derive(Deserialize)]
pub struct AnalyzeRequest {
    pub file_path: String,
}

// Helper: Convert ALL Excel Sheets to CSV String
fn excel_bytes_to_csv(bytes: Vec<u8>) -> Result<String, String> {
    let cursor = Cursor::new(bytes);
    let mut workbook = Xlsx::new(cursor).map_err(|e| e.to_string())?;
    
    // Ambil daftar nama sheet untuk iterasi
    let sheet_names = workbook.sheet_names().to_owned();
    
    let mut combined_output = String::new();

    for name in sheet_names {
        // --- PERBAIKAN DI SINI (Hapus 'Some') ---
        // Jika workbook.worksheet_range mengembalikan Result, kita langsung match Ok(range)
        if let Ok(range) = workbook.worksheet_range(&name) {
            combined_output.push_str(&format!("\n--- SHEET: {} ---\n", name));
            
            for row in range.rows() {
                let row_values: Vec<String> = row.iter().map(|c| match c {
                    Data::String(s) => format!("\"{}\"", s.replace("\"", "\"\"")), 
                    Data::Float(f) => f.to_string(),
                    Data::Int(i) => i.to_string(),
                    Data::Bool(b) => b.to_string(),
                    Data::DateTime(d) => d.to_string(),
                    Data::DateTimeIso(d) => d.to_string(), 
                    Data::DurationIso(d) => d.to_string(), 
                    Data::Empty => "".to_string(),
                    Data::Error(e) => format!("ERR: {:?}", e),
                    _ => "".to_string(), 
                }).collect();
                
                combined_output.push_str(&row_values.join(","));
                combined_output.push('\n');
            }
        }
    }

    if combined_output.is_empty() {
        return Err("File Excel kosong atau tidak terbaca di semua sheet".to_string());
    }
    
    Ok(combined_output)
}

pub async fn analyze_document_stream(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AnalyzeRequest>,
) -> impl IntoResponse {
    
    let relative_path = payload.file_path.trim_start_matches("/public/");
    let file_path = Path::new("media").join(relative_path);
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    // 1. Baca File
    let file_bytes = match fs::read(&file_path).await {
        Ok(b) => b,
        Err(e) => return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().data(format!("ERR_FILE: Gagal membaca file: {}", e)))
        ])).into_response(),
    };

    // 2. Konversi ke Text (Sekarang membaca semua sheet)
    let file_content_result = if ["xlsx", "xls"].contains(&extension.as_str()) {
        let bytes_clone = file_bytes.clone(); 
        task::spawn_blocking(move || excel_bytes_to_csv(bytes_clone)).await.unwrap_or(Err("Thread Error".to_string()))
    } else if ["csv", "txt", "json", "md", "html"].contains(&extension.as_str()) {
        String::from_utf8(file_bytes)
            .map_err(|_| "File non-UTF8".to_string())
    } else {
        return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().data(format!("ERR_FMT: Format .{} belum didukung.", extension)))
        ])).into_response();
    };

    let file_content = match file_content_result {
        Ok(c) => c,
        Err(e) => return Sse::new(futures::stream::iter(vec![
            Ok::<Event, Infallible>(Event::default().data(format!("ERR_PARSE: {}", e)))
        ])).into_response(),
    };

    // 3. Truncate Content (50k chars untuk memuat Neraca & Laba Rugi)
    let truncated_content = if file_content.len() > 50000 {
        format!("{}... (truncated)", &file_content[..50000])
    } else {
        file_content
    };

    // 4. IMPROVED PROMPT (Fuzzy Logic / Semantic Matching)
    let system_prompt = r#"You are a high-precision Financial Data Extraction Engine.
Your task is to parse financial report data (CSV/Text) from MULTIPLE SHEETS and extract key metrics into a strict JSON format.

### EXTRACTION RULES:
1. **Target Data:** Scan the entire document. The data is split across sections (e.g., General Info, Financial Position, Profit Loss).
   - Locate the **Current Reporting Period** column (look for dates like "2025-03-31" or terms like "Current Period").
   - Ignore "Prior Year" or "Beginning" columns.
2. **Number Formatting:**
   - Extract numbers AS IS (raw values).
   - Do NOT multiply by millions/thousands automatically.
   - Handle parentheses `(123)` as negative `-123`.
   - Remove thousand separators (e.g., `1.234,56` -> `1234.56`).

### SMART FIELD MAPPING (Fuzzy Logic):
Do NOT look for exact string matches. Accounting terms vary by company. Use semantic understanding to find the closest meaning:
1. **`nama_entitas`**: Look for "Nama Perusahaan", "Entitas", "Entity Name", or the company name mentioned in the header.
2. **`mata_uang`**: Look for "Currency", "Mata Uang Pelaporan", "Disajikan dalam..." (e.g., IDR, USD).
3. **`satuan_angka`**: Look for scale indicators like "Dalam Jutaan", "In Millions", "Ribuan", "Thousands", "Rounding".
4. **`total_aset`**: Find the Grand Total of Assets.
   - Keywords: "Jumlah Aset", "Total Aset", "Total Aktiva", "Total Harta", "Jumlah Kekayaan".
5. **`total_liabilitas`**: Find the Grand Total of Liabilities.
   - Keywords: "Jumlah Liabilitas", "Total Liabilitas", "Total Kewajiban", "Jumlah Utang", "Total Hutang".
6. **`total_ekuitas`**: Find the Grand Total of Equity.
   - Keywords: "Jumlah Ekuitas", "Total Ekuitas", "Total Modal", "Ekuitas Bersih".
7. **`laba_bersih`**: Find the final bottom-line profit.
   - Keywords: "Laba Bersih", "Laba Tahun Berjalan", "Laba Periode Berjalan", "Net Income", "Profit for the period", "Comprehensive Income attributable to parent" (if Net Income is missing).

### OUTPUT SCHEMA (Strict JSON):
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
}
For `data_keuangan_lain`, extract 5-10 key items (e.g., Cash, Revenue, Cost of Revenue) using their original names found in the doc."#;
    
    let user_prompt = format!(r#"ANALYZE THIS FINANCIAL REPORT DATA (Multiple Sheets Combined):
---
{}
---
Output ONLY the JSON object. Detect the fields based on meaning, not just exact words."#, truncated_content);

    let api_key = state.kolosal_key.clone();
    let url = "https://api.kolosal.ai/v1/chat/completions";
    let client = reqwest::Client::new();

    let request_body = json!({
        "model": "Kimi K2",
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "stream": true,
        "temperature": 0.1, // Tetap rendah agar AI fokus pada data, bukan kreatif mengarang
        "response_format": { "type": "json_object" },
        "max_tokens": 3000
    });

    let req_builder = client
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", api_key))
        .json(&request_body);

    let stream = async_stream::stream! {
        let response = match req_builder.send().await {
            Ok(r) => r,
            Err(e) => {
                yield Ok::<Event, Infallible>(Event::default().data(format!("ERR_CONN: {}", e)));
                return;
            }
        };

        if !response.status().is_success() {
             let err = response.text().await.unwrap_or_default();
             yield Ok::<Event, Infallible>(Event::default().data(format!("ERR_API: {}", err)));
             return;
        }

        let mut byte_stream = response.bytes_stream();
        while let Some(chunk) = byte_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                        yield Ok::<Event, Infallible>(Event::default().data(text));
                    }
                }
                Err(e) => yield Ok::<Event, Infallible>(Event::default().data(format!("ERR_STREAM: {}", e))),
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response()
}