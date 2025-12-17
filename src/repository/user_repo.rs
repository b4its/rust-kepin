use mongodb::{Database, Collection, bson::doc};
use crate::models::user::User;

pub struct UserRepository {
    pub collection: Collection<User>,
}

impl UserRepository {
    pub fn new(db: &Database) -> Self {
        UserRepository {
            collection: db.collection("users"),
        }
    }

    pub async fn find_by_email(&self, email: &str) -> Option<User> {
        self.collection.find_one(doc! { "email": email }, None).await.ok().flatten()
    }

    pub async fn create_user(&self, user: User) -> mongodb::error::Result<()> {
        self.collection.insert_one(user, None).await?;
        Ok(())
    }
}