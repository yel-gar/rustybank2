use crate::db::{Database, UserId};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ConnectionState {
    pub db: Arc<Database>,
    pub user_id: RwLock<Option<UserId>>,
}

impl ConnectionState {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            user_id: RwLock::new(None),
        }
    }
}
