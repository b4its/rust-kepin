use mongodb::{Database, Collection, bson::{doc, oid::ObjectId}};
use crate::models::upload::UserUpload;
use futures::TryStreamExt;

#[derive(Clone)]
pub struct UploadRepository {
    pub collection: Collection<UserUpload>,
}

impl UploadRepository {
    pub fn new(db: &Database) -> Self {
        UploadRepository {
            collection: db.collection("user_uploads"),
        }
    }

    pub async fn create_upload(&self, upload: UserUpload) -> mongodb::error::Result<()> {
        self.collection.insert_one(upload, None).await?;
        Ok(())
    }

    pub async fn find_by_user(&self, user_id: &str) -> mongodb::error::Result<Vec<UserUpload>> {
        let filter = doc! { "user_id": user_id };
        let mut cursor = self.collection.find(filter, None).await?;
        
        let mut uploads = Vec::new();
        while let Some(doc) = cursor.try_next().await? {
            uploads.push(doc);
        }
        Ok(uploads)
    }

    // 1. Cari berdasarkan ID (Penting untuk mendapatkan nama file sebelum dihapus)
    pub async fn find_by_id(&self, id: &str) -> mongodb::error::Result<Option<UserUpload>> {
        let oid = ObjectId::parse_str(id).map_err(|_| mongodb::error::Error::custom("Invalid ID"))?;
        self.collection.find_one(doc! { "_id": oid }, None).await
    }

    pub async fn count_by_user(&self, user_id: &str) -> mongodb::error::Result<u64> {
        // Filter sesuai dengan field di model UserUpload Anda
        let filter = doc! { "user_id": user_id };
        
        // Mengembalikan jumlah dokumen saja
        let count = self.collection.count_documents(filter, None).await?;
        Ok(count)
    }

    pub async fn get_uploads_stats(&self, user_id: &str) -> mongodb::error::Result<serde_json::Value> {
        let pipeline = vec![
            // 1. Match user_id
            doc! { "$match": { "user_id": user_id } },
            
            // 2. Lookup/Join dengan financial_reports
            doc! {
                "$lookup": {
                    "from": "financial_reports",
                    "let": { "upload_id": { "$toString": "$_id" } },
                    "pipeline": [
                        { "$match": { "$expr": { "$eq": ["$id_userupload", "$$upload_id"] } } }
                    ],
                    "as": "matched_financial"
                }
            },

            // 3. Hitung Total dan Analyzed
            doc! {
                "$group": {
                    "_id": null,
                    "total": { "$sum": 1 },
                    "analyzed": { 
                        "$sum": { 
                            "$cond": [ { "$gt": [ { "$size": "$matched_financial" }, 0 ] }, 1, 0 ] 
                        } 
                    }
                }
            }
        ];

        let mut cursor = self.collection.aggregate(pipeline, None).await?;
        
        if let Some(result) = cursor.try_next().await? {
            let total = result.get_i32("total").unwrap_or(0);
            let analyzed = result.get_i32("analyzed").unwrap_or(0);
            let pending = total.saturating_sub(analyzed);

            Ok(serde_json::json!({
                "total": total,
                "analyzed": analyzed,
                "pending": pending
            }))
        } else {
            Ok(serde_json::json!({ "total": 0, "analyzed": 0, "pending": 0 }))
        }
    }


    // 2. Hapus Record dari DB
    pub async fn delete(&self, id: &str) -> mongodb::error::Result<u64> {
        let oid = ObjectId::parse_str(id).map_err(|_| mongodb::error::Error::custom("Invalid ID"))?;
        let result = self.collection.delete_one(doc! { "_id": oid }, None).await?;
        Ok(result.deleted_count)
    }
}