use sqlx::{mysql::MySqlPoolOptions, sqlite::SqlitePoolOptions, MySqlPool, SqlitePool};

pub struct Database {
    pub mysql: MySqlPool,
    pub sqlite: SqlitePool,
}

impl Database {
    pub async fn new(mysql_url: &str, sqlite_url: &str) -> Self {
        let mysql = MySqlPoolOptions::new()
            .connect(mysql_url)
            .await
            .expect("Failed to connect to MySQL");

        let sqlite = SqlitePoolOptions::new()
            .connect(sqlite_url)
            .await
            .expect("Failed to connect to SQLite");

        Self { mysql, sqlite }
    }
}
