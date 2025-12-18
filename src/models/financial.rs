use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;
use chrono::{DateTime, Utc};

// Struktur JSON dari AI
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FinancialData {
    pub nama_entitas: String,
    pub periode_laporan: String,
    pub mata_uang: String,
    pub satuan_angka: String,
    pub total_aset: f64,
    pub total_liabilitas: f64,
    pub total_ekuitas: f64,
    pub laba_bersih: f64,
    pub data_keuangan_lain: Vec<FinancialItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FinancialItem {
    pub keterangan: String,
    pub nilai: f64,
}

// Struktur Dokumen Database
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FinancialRecord {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    
    pub user_id: String,       // ID User
    pub id_userupload: String, 
    pub source_file: String,   // Path file
    
    #[serde(flatten)]          // Data AI digabung ke root dokumen
    pub data: FinancialData, 

    #[serde(with = "mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
}