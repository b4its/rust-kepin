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

    // 2. Hapus Record dari DB
    pub async fn delete(&self, id: &str) -> mongodb::error::Result<u64> {
        let oid = ObjectId::parse_str(id).map_err(|_| mongodb::error::Error::custom("Invalid ID"))?;
        let result = self.collection.delete_one(doc! { "_id": oid }, None).await?;
        Ok(result.deleted_count)
    }
}