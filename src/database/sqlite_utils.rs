// src/db.rs

use bcrypt::{hash, verify, DEFAULT_COST};
// use sqlx::{params, Connection, Result};
use sqlx::{Executor, FromRow, SqliteConnection, SqlitePool}; // 或使用 &mut SqliteConnection 作为 conn
#[derive(Debug, FromRow)]
pub struct User {
    pub id: i64,
    pub phone: String,
    pub password_hash: String,
}

pub async fn init_db(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            phone TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL
        )",
    )
    .execute(conn)
    .await?;

    Ok(())
}

pub async fn create_user(
    conn: &mut SqliteConnection,
    phone: &str,
    password: &str,
) -> Result<(), sqlx::Error> {
    let password_hash = hash(password, DEFAULT_COST).unwrap();
    // conn.execute(
    //     "INSERT INTO users (phone, password_hash) VALUES (?1, ?2)",
    //     params![phone, password_hash],
    // )?;

    sqlx::query("INSERT INTO users (phone, password_hash) VALUES (?, ?)")
        .bind(phone)
        .bind(password_hash)
        .execute(conn)
        .await?;

    Ok(())
}

pub async fn get_user_by_phone(
    conn: &mut SqliteConnection,
    phone: &str,
) -> Result<Option<User>, sqlx::Error> {
    let user =
        sqlx::query_as::<_, User>("SELECT id, phone, password_hash FROM users WHERE phone = ?")
            .bind(phone)
            .fetch_optional(conn) // 用于获取 Option<T>
            .await?;

    Ok(user)
}

pub async fn verify_user_password(
    conn: &mut SqliteConnection,
    phone: &str,
    password: &str,
) -> Result<bool, sqlx::Error> {
    if let Some(user) = get_user_by_phone(conn, phone).await? {
        Ok(verify(password, &user.password_hash).unwrap_or(false))
    } else {
        Ok(false)
    }
}
