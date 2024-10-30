use actix_web::{web, App, HttpServer, Responder};
use chrono::{DateTime, Utc};
use mysql::prelude::*;
use mysql::*;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlPool, FromRow, Result};

// 定义数据结构
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct UserAccount {
    pub user_id: i32,
    pub email: String,
    pub registration_time: DateTime<Utc>,
    pub last_login_time: Option<DateTime<Utc>>,
    pub last_access_time: Option<DateTime<Utc>>,
    pub token: Option<String>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct UserOauthAccount {
    pub id: i32,
    pub user_id: i32,
    pub provider: i32, // 0-TOPAL, 1-Google, 2-GitHub
    pub external_user_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct UserPincode {
    pub id: i32,
    pub pincode: String,
    pub created_at: DateTime<Utc>,
    pub email: String,
}

pub struct DatabaseManager {
    pub conn_string: String,
    pub pool: MySqlPool,
}

pub async fn new(database_url: &str) -> Result<DatabaseManager, sqlx::Error> {
    let conn_string = database_url.to_string();
    let pool = match MySqlPool::connect(&conn_string).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Failed to create pool: {}", e);
            return Err(sqlx::Error::from(e));
        }
    };
    Ok(DatabaseManager {
        conn_string: database_url.to_string(),
        pool,
    })
}

impl DatabaseManager {
    pub async fn connect(&self) -> Pool {
        Pool::new(Opts::from_url(&self.conn_string).unwrap()).unwrap()
    }

    pub async fn create_table(&self) -> Result<()> {
        // let mut conn = Pool::new(&self.conn_string).unwrap().get_conn().unwrap();
        let mut conn = Pool::new(Opts::from_url(&self.conn_string).unwrap())
            .unwrap()
            .get_conn()
            .unwrap();

        // conn.query_drop(format!(
        //     "CREATE TABLE IF NOT EXISTS {} (id INT, name TEXT, PRIMARY KEY(id))",
        //     table_name
        // ))
        // .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS USER_ACCOUNT (
                user_id INT PRIMARY KEY AUTO_INCREMENT,
                email VARCHAR(255) NOT NULL UNIQUE,
                registration_time DATETIME NOT NULL,
                last_login_time DATETIME,
                last_access_time DATETIME,
                token VARCHAR(255)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS USER_PINCODE (
                id INT PRIMARY KEY AUTO_INCREMENT,
                pincode VARCHAR(255) NOT NULL,
                created_at DATETIME NOT NULL,
                email VARCHAR(255) NOT NULL UNIQUE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS USER_OAUTH_ACCOUNT (
                id INT PRIMARY KEY AUTO_INCREMENT,
                user_id INT NOT NULL,
                provider INT NOT NULL,
                external_user_id VARCHAR(255) NOT NULL,
                access_token VARCHAR(255) NOT NULL,
                refresh_token VARCHAR(255),
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL,
                FOREIGN KEY (user_id) REFERENCES USER_ACCOUNT(user_id),
                UNIQUE KEY unique_provider_external (provider, external_user_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // UserAccount CRUD operations
    pub async fn create_update_user(&self, email: &str) -> Result<(UserAccount, bool)> {
        let now = Utc::now();

        // Generate an API token using a common method in Rust
        let topai_token = if let Ok(Some(existing_user)) = self.get_user_by_email(email).await {
            if let Some(existing_token) = existing_user.token {
                let days_since_registration = (now - existing_user.registration_time).num_days();
                if days_since_registration <= 14 {
                    existing_token
                } else {
                    thread_rng()
                        .sample_iter(&Alphanumeric)
                        .take(30)
                        .map(char::from)
                        .collect()
                }
            } else {
                thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(30)
                    .map(char::from)
                    .collect()
            }
        } else {
            thread_rng()
                .sample_iter(&Alphanumeric)
                .take(30)
                .map(char::from)
                .collect()
        };

        // First perform the insert/update
        let result = sqlx::query(
            r#"
            INSERT INTO USER_ACCOUNT (email, registration_time, last_login_time, last_access_time, token)
            VALUES (?, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                user_id = LAST_INSERT_ID(user_id),
                last_login_time = VALUES(last_login_time),
                last_access_time = VALUES(last_access_time),
                token = VALUES(token)
            "#,
        )
        .bind(email)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(topai_token)
        .execute(&self.pool).await?;

        // Determine if it was an insert or update
        let was_insert = result.rows_affected() == 1;

        // Get the ID of the affected row
        let id = result.last_insert_id();

        // Then select the updated/inserted row using the ID
        let user = sqlx::query_as::<_, UserAccount>("SELECT * FROM USER_ACCOUNT WHERE user_id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok((user, was_insert))
    }

    pub async fn get_user_by_id(&self, user_id: i32) -> Result<Option<UserAccount>> {
        let result =
            sqlx::query_as::<_, UserAccount>("SELECT * FROM USER_ACCOUNT WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(result)
    }

    pub async fn update_user_login(&self, user_id: i32, token: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE USER_ACCOUNT
            SET last_login_time = ?, token = ?
            WHERE user_id = ?
            "#,
        )
        .bind(now)
        .bind(token)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    //get user by token
    pub async fn get_user_by_token(&self, token: &str) -> Result<Option<UserAccount>> {
        let result = sqlx::query_as::<_, UserAccount>("SELECT * FROM USER_ACCOUNT WHERE token = ?")
            .bind(token)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    //get user by email
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<UserAccount>> {
        let result = sqlx::query_as::<_, UserAccount>("SELECT * FROM USER_ACCOUNT WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    pub async fn delete_user(&self, user_id: i32) -> Result<()> {
        sqlx::query("DELETE FROM USER_ACCOUNT WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // UserOauthAccount CRUD operations
    pub async fn create_oauth_account(
        &self,
        user_id: i32,
        provider: i32,
        external_user_id: &str,
        access_token: &str,
        refresh_token: Option<&str>,
    ) -> Result<UserOauthAccount> {
        let now = Utc::now();
        let result = sqlx::query_as::<_, UserOauthAccount>(
            r#"
            INSERT INTO USER_OAUTH_ACCOUNT (
                user_id, provider, external_user_id,
                access_token, refresh_token, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(provider)
        .bind(external_user_id)
        .bind(access_token)
        .bind(refresh_token)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_oauth_account(
        &self,
        provider: i32,
        external_user_id: &str,
    ) -> Result<Option<UserOauthAccount>> {
        let result = sqlx::query_as::<_, UserOauthAccount>(
            r#"
            SELECT * FROM USER_OAUTH_ACCOUNT
            WHERE provider = ? AND external_user_id = ?
            "#,
        )
        .bind(provider)
        .bind(external_user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn update_oauth_tokens(
        &self,
        id: i32,
        access_token: &str,
        refresh_token: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE USER_OAUTH_ACCOUNT
            SET access_token = ?, refresh_token = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(access_token)
        .bind(refresh_token)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_oauth_account(&self, id: i32) -> Result<()> {
        sqlx::query("DELETE FROM USER_OAUTH_ACCOUNT WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    //user pin code operations
    // insert pincode by user if exist then update
    pub async fn insert_update_pincode(&self, pincode: &str, email: &str) -> Result<UserPincode> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            INSERT INTO USER_PINCODE (pincode, created_at, email)
            VALUES (?, ?, ?)
            ON DUPLICATE KEY UPDATE
                created_at = VALUES(created_at),
                pincode = VALUES(pincode)
            "#,
        )
        .bind(pincode)
        .bind(now)
        .bind(email)
        .execute(&self.pool)
        .await?;

        let id = result.last_insert_id();

        let pincode = sqlx::query_as::<_, UserPincode>("SELECT * FROM USER_PINCODE WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        Ok(pincode)
    }

    //get pincode by email
    pub async fn get_pincode_by_email(&self, email: &str) -> Result<Option<UserPincode>> {
        let result = sqlx::query_as::<_, UserPincode>("SELECT * FROM USER_PINCODE WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    pub async fn invalidate_token(&self, token: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE USER_ACCOUNT
            SET token = NULL
            WHERE token = ?
            "#,
        )
        .bind(token)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// async fn index() -> impl Responder {
//     // Create the database manager
//     let db_manager = DatabaseManager::new("mysql://root:password@localhost:3306/test".to_string());
//     db_manager.
//     // Connect to the database
//     // let pool = db_manager.connect(db_manager.conn_string).await;

//     // Get a connection from the pool
//     // let mut conn = pool.get_conn().unwrap();

//     // Do something with the connection
//     // e.g., execute a simple query
//     let row: Option<u8> = conn.query_first("SELECT 1").unwrap();
//     assert_eq!(row, Some(1));

//     //// Create a new
//     format!("Connected to MySQL")
// }
