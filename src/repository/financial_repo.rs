use mongodb::{Database, Collection, options::ReplaceOptions};
use mongodb::bson::doc;
use futures::stream::TryStreamExt;
use crate::models::financial::FinancialRecord;

#[derive(Clone)]
pub struct FinancialRepository {
    pub collection: Collection<FinancialRecord>,
}

impl FinancialRepository {
    pub fn new(db: &Database) -> Self {
        FinancialRepository {
            collection: db.collection("financial_reports"),
        }
    }

    pub async fn save(&self, record: FinancialRecord) -> mongodb::error::Result<()> {
        let filter = doc! { "id_userupload": &record.id_userupload };
        
        let options = ReplaceOptions::builder().upsert(true).build();

        self.collection.replace_one(filter, record, options).await?;
        Ok(())
    }

    pub async fn find_by_user(&self, user_id: &str) -> mongodb::error::Result<Vec<FinancialRecord>> {
        let filter = doc! { "user_id": user_id };
        let find_options = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();

        let mut cursor = self.collection.find(filter, find_options).await?;
        let mut results = Vec::new();
        while let Some(record) = cursor.try_next().await? {
            results.push(record);
        }
        Ok(results)
    }

    pub async fn count_by_user(&self, user_id: &str) -> mongodb::error::Result<u64> {
        // Menghitung jumlah dokumen di 'financial_reports' milik user_id ini
        let filter = doc! { "user_id": user_id };
        self.collection.count_documents(filter, None).await
    }

    pub async fn delete_by_upload_id(&self, upload_id: &str) -> mongodb::error::Result<u64> {
        // Query untuk mencari dokumen dengan id_userupload yang cocok
        let filter = doc! { "id_userupload": upload_id };
        
        let result = self.collection.delete_many(filter, None).await?;
        
        println!("Deleted {} financial records for upload_id: {}", result.deleted_count, upload_id);
        Ok(result.deleted_count)
    }
}