// src/api.rs

// use UserAgent::communication::sms;
use crate::communication::sms;
use crate::database::sqlite_utils as db;
use actix_session::Session;
use actix_web::{cookie::Cookie, get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
// use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, SqliteConnection, SqlitePool};

const JWT_SECRET: &[u8] = b"your_super_secret_key"; // 生产环境请安全存储

#[derive(Deserialize)]
pub struct SendCodeRequest {
    phone: String,
}
#[post("/send_code")]
async fn send_code(data: web::Data<SqlitePool>, req: web::Json<SendCodeRequest>) -> impl Responder {
    let code = sms::generate_code();
    sms::save_code(&req.phone, &code);

    println!("模拟发送短信到 {}: 验证码是 {}", req.phone, code);

    let mut conn = data.acquire().await.unwrap();
    if db::get_user_by_phone(&mut conn, &req.phone)
        .await
        .unwrap()
        .is_none()
    {
        let _ = db::create_user(&mut conn, &req.phone, &code);
    }

    HttpResponse::Ok().body("验证码已发送")
}

#[derive(Deserialize)]
pub struct LoginByCodeRequest {
    phone: String,
    code: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[post("/login_by_code")]
async fn login_by_code(
    data: web::Data<SqliteConnection>,
    req: web::Json<LoginByCodeRequest>,
) -> impl Responder {
    if sms::verify_code(&req.phone, &req.code) {
        let exp = (Utc::now() + Duration::hours(2)).timestamp() as usize;
        let claims = Claims {
            sub: req.phone.clone(),
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(JWT_SECRET),
        )
        .unwrap();
        HttpResponse::Ok().json(serde_json::json!({ "token": token }))
    } else {
        HttpResponse::Unauthorized().body("验证码错误")
    }
}

#[derive(Deserialize)]
pub struct PasswordLoginRequest {
    phone: String,
    password: String,
}

#[post("/login_by_password")]
async fn login_by_password(
    data: web::Data<SqlitePool>,
    req: web::Json<PasswordLoginRequest>,
    session: Session,
) -> impl Responder {
    let mut conn = data.acquire().await.unwrap();
    if db::verify_user_password(&mut conn, &req.phone, &req.password)
        .await
        .unwrap_or(false)
    {
        session.insert("user_phone", &req.phone).unwrap();

        let cookie = Cookie::build("user_phone", req.phone.clone())
            .http_only(true)
            .secure(true)
            .finish();

        HttpResponse::Ok().cookie(cookie).body("登录成功")
    } else {
        HttpResponse::Unauthorized().body("手机号或密码错误")
    }
}

#[post("/logout")]
async fn logout(session: Session) -> impl Responder {
    session.purge();
    HttpResponse::Ok().body("已登出")
}

#[get("/me")]
async fn me(session: Session, req: HttpRequest) -> impl Responder {
    session.purge();
    HttpResponse::Ok().body("已登出")
}

// #[derive(Deserialize)]
// pub struct SendsmsCodeRequest {
//     phone: String
// }

// #[post("sendsmscode")]
// async fn sendsmscode()
