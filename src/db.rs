use crate::cli::Args;
use anyhow::anyhow;
use sqlx::migrate::MigrateDatabase;
use sqlx::{Sqlite, SqlitePool, query};
use tracing::info;

pub type UserId = i64;
pub type ItemId = i64;
pub type Money = i64;

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(args: &Args) -> anyhow::Result<Self> {
        info!("Connecting to database");
        let url = args
            .db_file
            .to_str()
            .ok_or(anyhow!("Failed to parse database file path"))?;
        Sqlite::create_database(url).await?;

        let pool = SqlitePool::connect(url).await?;
        info!("Database connected");

        Ok(Self { pool })
    }

    pub async fn get_balance(&self, user_id: &UserId) -> Option<Money> {
        if let Ok(r) = query!("SELECT money FROM users WHERE id = $1", user_id)
            .fetch_one(&self.pool)
            .await
        {
            return Some(r.money);
        }
        None
    }

    pub async fn get_user_hash_and_id(&self, username: &str) -> Option<(String, UserId)> {
        if let Ok(r) = query!(
            "SELECT id, password_hash FROM users WHERE username = $1",
            username
        )
        .fetch_one(&self.pool)
        .await
        {
            return Some((r.password_hash, r.id));
        }
        None
    }

    pub async fn insert_user(&self, username: &str, password_hash: String) -> anyhow::Result<()> {
        query!(
            "INSERT INTO users (username, password_hash) VALUES ($1, $2)",
            username,
            password_hash
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_user_can_work_at(&self, user_id: &UserId) -> Option<i64> {
        if let Ok(r) = query!("SELECT can_work_at FROM users WHERE id = $1", user_id)
            .fetch_one(&self.pool)
            .await
        {
            return Some(r.can_work_at);
        }
        None
    }

    pub async fn set_user_can_work_at(
        &self,
        user_id: &UserId,
        can_work_at: i64,
    ) -> anyhow::Result<()> {
        query!(
            "UPDATE users SET can_work_at = $2 WHERE id = $1",
            user_id,
            can_work_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn change_user_money(&self, user_id: &UserId, amount: Money) -> anyhow::Result<()> {
        query!(
            "UPDATE users SET money = money + $2 WHERE id = $1",
            user_id,
            amount
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_all_items(&self) -> anyhow::Result<Vec<(ItemId, String, Money)>> {
        Ok(
            query!("SELECT id, name AS 'name!', price FROM items ORDER BY id")
                .fetch_all(&self.pool)
                .await?
                .iter()
                .map(|r| (r.id, r.name.clone(), r.price))
                .collect(),
        )
    }

    pub async fn get_user_items(&self, user_id: &UserId) -> anyhow::Result<Vec<(String, String)>> {
        Ok(query!(
            "SELECT i.name, i.description FROM item_ownership io
            INNER JOIN items i on id = item_id
            WHERE io.user_id = $1",
            user_id
        )
        .fetch_all(&self.pool)
        .await?
        .iter()
        .map(|r| (r.name.clone(), r.description.clone()))
        .collect())
    }

    pub async fn get_item_price(&self, item_id: &ItemId) -> Option<Money> {
        if let Ok(r) = query!("SELECT price FROM items WHERE id = $1", item_id)
            .fetch_one(&self.pool)
            .await
        {
            return Some(r.price);
        }
        None
    }

    pub async fn add_user_item(&self, user_id: &UserId, item_id: &ItemId) -> anyhow::Result<()> {
        query!(
            "INSERT INTO item_ownership (item_id, user_id) VALUES ($1, $2)",
            item_id,
            user_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_user_item(&self, user_id: &UserId, item_id: &ItemId) -> anyhow::Result<()> {
        query!(
            "DELETE FROM item_ownership WHERE ROWID = (
                SELECT ROWID FROM item_ownership WHERE user_id = $1 AND item_id = $2
                LIMIT 1
            )",
            user_id,
            item_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_user_has_item(
        &self,
        user_id: &UserId,
        item_id: &ItemId,
    ) -> anyhow::Result<bool> {
        Ok(query!("SELECT EXISTS(SELECT 1 FROM item_ownership WHERE user_id = $1 AND item_id = $2) AS 'ex:bool'", user_id, item_id)
            .fetch_one(&self.pool)
            .await?.ex)
    }
}
