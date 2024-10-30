use actix_cors::Cors;
use actix_session::{Session, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::error::ErrorInternalServerError;
use actix_web::{
    get, http::header, middleware::Logger, post, web, App, Error, HttpRequest, HttpResponse,
    HttpServer, Responder,
};

use actix_web_httpauth::headers::authorization::{Authorization, Bearer};

use mysql::params;
use oauth2::TokenResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use dotenv;
use log::*;
use serde_json::json;
use std::env;
// email part
use aws_config;
use aws_sdk_sesv2::error::BuildError;

use aws_sdk_sesv2::{
    types::{Destination, EmailContent, Template},
    Client,
};
use reqwest;
use std::collections::HashMap;

use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, RedirectUrl, Scope, TokenUrl,
};

use UserAgent::communication::aws_utils::{self, EmailManager};
// use DatabaseManager
use UserAgent::database::mysql_utils::{self, UserPincode};
//mq
use UserAgent::mq::kafka;

use std::time::Duration;

// const MarketingServerUrl: &str = "topai-marketing-server.demo-ray.svc.cluster.local/api/v1/voucher/exchange:80";
const MarketingServerUrl: &str = "159.135.196.73:32756";
#[derive(Serialize, Deserialize)]
struct UserInfo {
    name: String,
    email: String,
}

pub async fn generate_tokens(
    // web::Json(params): web::Json<PinCodeVerification>,
    email: String,
    voucher_code: String,
    db: web::Data<mysql_utils::DatabaseManager>,
    kafka_handler: web::Data<kafka::KafkaHandler>,
) -> Result<String, sqlx::Error> {
    match db.create_update_user(email.as_str()).await {
        Ok((user_account, new_user)) => {
            println!("start create_update_user {:?}", new_user);

            //sync to vouch
            if !voucher_code.is_empty() {
                // TODO: Upload voucher code to specified URL
                // Implementation needed to post voucher_code to endpoint
                if let Err(e) = exchange_voucher_code(user_account.user_id, voucher_code).await {
                    eprintln!("Failed to exchange voucher code: {}", e);
                }
            }
            if new_user {
                //sendmsg to kafka

                let mut map: HashMap<String, Value> = HashMap::new();
                map.insert("user_id".to_string(), json!(user_account.user_id));
                map.insert("type".to_string(), json!("user"));
                match kafka_handler.send_message("user", map).await {
                    Ok(_) => {
                        println!("send kafka success");
                    }
                    Err(e) => {
                        println!("send kafka failed: {}", e);
                    }
                }
            }

            return Ok(user_account.token.unwrap());
        }
        Err(e) => {
            return Err(e);
        }
    };
}

//pin code verification
pub async fn get_or_generate_pincode(
    email: &str,
    db: web::Data<mysql_utils::DatabaseManager>,
) -> Result<String, sqlx::Error> {
    // Check if pincode exists for this email
    match db.get_pincode_by_email(email).await {
        Ok(Some(user_pincode)) => {
            let now = chrono::Utc::now();
            let duration = now.signed_duration_since(user_pincode.created_at);

            // If pincode was created less than 10 minutes ago, return existing
            if duration.num_minutes() < 10 {
                return Ok(user_pincode.pincode.clone());
            } else {
                let new_pincode = format!("{:08}", rand::random::<u32>() % 100000000);
                match db.insert_update_pincode(new_pincode.as_str(), email).await {
                    Ok(_) => (),
                    Err(e) => return Err(e),
                }

                // Send email with new pincode
                // match email_manager.send_email_to(email).await {
                //     Ok(_) => {
                //         println!("Email sent successfully");
                //     }
                //     Err(e) => {
                //         error!("Failed to send email: {}", e);
                //         return Err(sqlx::Error::Protocol(format!(
                //             "Failed to send email: {}",
                //             e
                //         )));
                //         // return Err(anyhow::Error::from("Failed to send email"));
                //     }
                // };
                Ok(new_pincode)
            }
        }
        Ok(None) => {
            // No existing pincode found, will generate new one below
            let new_pincode = format!("{:08}", rand::random::<u32>() % 100000000);
            match db.insert_update_pincode(new_pincode.as_str(), email).await {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }?;
            Ok(new_pincode)
        }
        Err(e) => return Err(e),
    }
}
//
pub async fn exchange_voucher_code(user_id: i32, voucher_code: String) -> Result<(), Error> {
    let client = reqwest::Client::new();

    let marketing_server_url =
        env::var("MARKETING_SERVER_URL").unwrap_or_else(|_| MarketingServerUrl.to_string());

    println!("current marketing url {}", marketing_server_url);
    let response = client
        .post(format!(
            "http://{}/api/v1/voucher/exchange",
            marketing_server_url
        ))
        .header("Content-Type", "application/json")
        .body(
            json!({
                "userId": user_id,
                "voucherCode": voucher_code
            })
            .to_string(),
        )
        .send()
        .await;

    match response {
        Ok(res) => {
            if !res.status().is_success() {
                println!("Failed to upload voucher code: {}", res.status());
                return Err(ErrorInternalServerError(res.status().to_string()));
            }
        }
        Err(e) => {
            println!("Error uploading voucher code: {}", e);
            return Err(ErrorInternalServerError(e.to_string()));
        }
    }

    return Ok(());
}
// exchange voucher code

//github oauth client
fn create_github_oauth_client() -> BasicClient {
    let github_client_id = ClientId::new(
        env::var("GITHUB_CLIENT_ID").expect("Missing GITHUB_CLIENT_ID environment variable."),
    );
    let github_client_secret = ClientSecret::new(
        env::var("GITHUB_CLIENT_SECRET")
            .expect("Missing GITHUB_CLIENT_SECRET environment variable."),
    );

    let auth_url = AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
        .expect("Invalid github authorization endpoint URL");
    let token_url = TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
        .expect("Invalid github token endpoint URL");

    BasicClient::new(
        github_client_id,
        Some(github_client_secret),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(
        RedirectUrl::new("http://localhost:5173/auth/callback/github".to_string())
            .expect("Invalid redirect URL"),
    )
}

//google oauth client
fn create_google_oauth_client() -> BasicClient {
    let google_client_id = ClientId::new(
        env::var("GOOGLE_CLIENT_ID").expect("Missing GOOGLE_CLIENT_ID environment variable."),
    );
    let google_client_secret = ClientSecret::new(
        env::var("GOOGLE_CLIENT_SECRET")
            .expect("Missing GOOGLE_CLIENT_SECRET environment variable."),
    );
    let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
        .expect("Invalid authorization endpoint URL");
    let token_url = TokenUrl::new("https://www.googleapis.com/oauth2/v3/token".to_string())
        .expect("Invalid token endpoint URL");

    BasicClient::new(
        google_client_id,
        Some(google_client_secret),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(
        RedirectUrl::new("http://localhost:5173/auth/callback/google".to_string())
            .expect("Invalid redirect URL"),
    )
}

#[derive(Deserialize, Debug)]
struct Auth2CallbackParameters {
    code: String,
    state: String,
}

#[post("/api/v1/user/oauth2callback/{provider}")]
async fn oauth2callback(
    path: web::Path<(String,)>,
    oauth_client: web::Data<BasicClient>,
    web::Json(params): web::Json<Auth2CallbackParameters>,
    // web::Query(params): web::Query<HashMap<String, String>>,
    db: web::Data<mysql_utils::DatabaseManager>,
    kafka_handler: web::Data<kafka::KafkaHandler>,
) -> Result<HttpResponse, Error> {
    println!("oauth2callback");
    println!("{:?}", params);

    let voucher_code = params.state;
    println!("voucher_code: {}", voucher_code);

    let provider = path.into_inner().0;
    match provider.as_str() {
        "google" => Ok(HttpResponse::Ok().body("test")),
        "github" => {
            let token = oauth_client
                .exchange_code(oauth2::AuthorizationCode::new(params.code))
                .request_async(oauth2::reqwest::async_http_client)
                .await
                .map_err(|_| ErrorInternalServerError("Failed to exchange code"))?;
            let user_info = get_github_emails(token.access_token().secret().as_str())
                .await
                .unwrap();

            let primary_email = user_info
                .iter()
                .find(|e| e.primary)
                .map(|e| &e.email)
                .unwrap_or(&user_info[0].email);
            println!("Got github user primary email: {}", primary_email);

            // db.create_update_user(primary_email.as_str()).await;
            match generate_tokens(primary_email.to_string(), voucher_code, db, kafka_handler).await
            {
                Ok(topai_token) => {
                    println!("start create update .......");
                    return Ok(HttpResponse::Ok().json(topai_token));
                }
                Err(e) => {
                    println!("Error creating or updating user: {}", e);
                    return Err(ErrorInternalServerError(e.to_string()));
                }
            }

            // Ok(HttpResponse::Ok().json(token))
        }
        _ => Ok(HttpResponse::Ok().body("CURRENT PROVIDER NOT SUPPORTED")),
    }
}

#[derive(Deserialize, Debug)]
struct EmailParameters {
    email: String,
}

#[post("/api/v1/user/auth/email/send_pincode")]
async fn send_email(
    web::Json(params): web::Json<EmailParameters>,
    db: web::Data<mysql_utils::DatabaseManager>,
    email_manager: web::Data<EmailManager>,
) -> impl Responder {
    // match get_or_generate_pincode(params.email.as_str(), db, email_manager).await {
    //     Ok(pincode) => {
    //         println!("Pincode: {}", pincode);
    //         HttpResponse::Ok().json(json!({
    //             "message": "PINCODE sent to email"
    //         }))
    //     }
    //     Err(e) => {
    //         println!("Error generating pincode: {}", e);
    //         // return ErrorInternalServerError(e.to_string());
    //         HttpResponse::InternalServerError().body(format!("Failed to send_pincode: {}", e))
    //     }
    // }
    // Send email with new pincode
    //
    let new_pincode = match get_or_generate_pincode(params.email.as_str(), db).await {
        Ok(pincode) => pincode,
        Err(e) => {
            error!("Error generating pincode: {}", e);
            return HttpResponse::InternalServerError()
                .body(format!("Failed to generate_pincode: {}", e));
        }
    };

    match email_manager
        .send_email_to(params.email.as_str(), new_pincode.as_str())
        .await
    {
        Ok(_) => {
            println!("Email sent successfully");
            HttpResponse::Ok().json(json!({
                "message": "PINCODE sent to email"
            }))
        }
        Err(e) => {
            error!("Failed to send email: {}", e);
            HttpResponse::InternalServerError().body(format!("Failed to send_pincode: {}", e))
            // return Err(anyhow::Error::from("Failed to send email"));
        }
    }
}

#[derive(Deserialize)]
struct ExInfo {
    voucher_code: String,
}

#[derive(Deserialize)]
struct PinCodeVerification {
    email: String,
    pin_code: String,
    ex_info: ExInfo,
}

/**
* email_login_register
**/
#[post("/api/v1/user/auth/email/login_register")]
async fn email_login_register(
    web::Json(params): web::Json<PinCodeVerification>,
    db: web::Data<mysql_utils::DatabaseManager>,
    kafka_handler: web::Data<kafka::KafkaHandler>,
) -> impl Responder {
    // For demonstration purposes, we assume the correct pin code is "12345678"
    //
    //8位纯数字的随机数
    // let correct_pin_code = format!("{:08}", rand::random::<u32>() % 100000000);
    // let correct_pin_code = "12345678";

    // get pincode from mysql and check if expired
    let pin_code = match db.get_pincode_by_email(params.email.as_str()).await {
        Ok(pincode) => match pincode {
            Some(user_pincode) => {
                let now = chrono::Utc::now();
                let duration = now.signed_duration_since(user_pincode.created_at);

                if duration.num_minutes() >= 10 {
                    return HttpResponse::BadRequest().body("Pincode expired");
                }
                user_pincode.pincode
            }
            None => {
                return HttpResponse::BadRequest().body("No pincode found for this email");
            }
        },
        Err(e) => {
            println!("Error getting pincode: {}", e);
            return HttpResponse::InternalServerError()
                .body(format!("Failed to get pincode: {}", e));
        }
    };

    // if params.pin_code != pin_code {
    //     return HttpResponse::Unauthorized().body("mismatch Invalid pin code");
    // }
    if params.pin_code != correct_pin_code {
        return HttpResponse::Unauthorized().json(json!({
            "message": "mismatch Invalid pin code"
        }));
    }

    println!("start create update .......");

    match generate_tokens(
        params.email.to_string(),
        params.ex_info.voucher_code.to_string(),
        db,
        kafka_handler,
    )
    .await
    {
        Ok(topai_token) => {
            println!("start create update .......");
            return HttpResponse::Ok().json(json!({
                "topai_token": topai_token
            }));
        }
        Err(e) => {
            println!("Error creating or updating user: {}", e);
            return HttpResponse::InternalServerError()
                .body(format!("Failed to create user: {}", e));
        }
    }
}

#[get("/api/v1/user/oauth/init/{provider}")]
async fn oauth_url(
    path: web::Path<(String,)>,
    web::Query(params): web::Query<HashMap<String, String>>,
) -> impl Responder {
    let voucher_code = params
        .get("voucher_code")
        .unwrap_or(&"".to_string())
        .to_string();
    println!("voucher_code: {}", voucher_code);

    let provider = path.into_inner().0;
    match provider.as_str() {
        "google" => {
            println!("match google......");
            let client = create_google_oauth_client();
            let (auth_url, _csrf_token) = client
                // .authorize_url(CsrfToken::new_random)
                .authorize_url(move || CsrfToken::new(voucher_code.clone()))
                .add_scope(Scope::new(
                    "https://www.googleapis.com/auth/userinfo.email".to_string(),
                ))
                .add_scope(Scope::new(
                    "https://www.googleapis.com/auth/userinfo.profile".to_string(),
                ))
                .url();

            return HttpResponse::Ok().body(auth_url.to_string());
        }
        "github" => {
            println!("match github......");
            let client = create_github_oauth_client();
            let csrf_state = voucher_code;
            let (auth_url, _csrf_token) = client
                .authorize_url(move || CsrfToken::new(csrf_state.clone()))
                .add_scope(Scope::new("user:email".to_string())) //.set_pkce_challenge(pkce_challenge)
                .url();

            println!("Open this URL in your browser:\n{}", auth_url);

            // return HttpResponse::Ok().body(auth_url.to_string());
            return HttpResponse::Ok().json(json!({
                "redirect_url": auth_url.to_string()
            }));
        }
        _ => return HttpResponse::BadRequest().body("Unsupported provider"),
    };

    // return HttpResponse::Ok().body("Unsupported error".to_string());
}

#[get("/api/v1/user/auth_google_url")]
async fn get_auth_url() -> impl Responder {
    let client = create_google_oauth_client();
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(
            "https://www.googleapis.com/auth/userinfo.email".to_string(),
        ))
        .add_scope(Scope::new(
            "https://www.googleapis.com/auth/userinfo.profile".to_string(),
        ))
        .url();

    HttpResponse::Ok().body(auth_url.to_string())
}

#[get("/api/v1/user/github_auth_url")]
async fn get_github_auth_url() -> impl Responder {
    let client = create_github_oauth_client();
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("user:email".to_string()))
        .add_scope(Scope::new("read:user".to_string()))
        .url();

    println!("Open this URL in your browser:\n{}", auth_url);

    HttpResponse::Ok().json(json!({
        "redirect_url": auth_url.to_string()
    }))
}

#[get("/api/v1/user/userinfo")]
async fn get_user() -> impl Responder {
    // 这里应该实现从session中获取用户信息的逻辑
    // 为了演示，我们返回一个模拟的用户
    let user = UserInfo {
        name: "John Doe".to_string(),
        email: "johndoe@example.com".to_string(),
    };
    HttpResponse::Ok().json(user)
}

#[get("/api/v1/user/getuserbyemail")]
async fn get_user_by_email(
    web::Query(params): web::Query<HashMap<String, String>>,
    db: web::Data<mysql_utils::DatabaseManager>,
) -> impl Responder {
    let email = match params.get("email") {
        Some(email) => email,
        None => return HttpResponse::BadRequest().body("Missing email parameter"),
    };
    match db.get_user_by_email(email.as_str()).await {
        Ok(user) => HttpResponse::Ok().json(user),
        Err(e) => {
            println!("Error getting user by email: {}", e);
            HttpResponse::InternalServerError().body(format!("Failed to get user: {}", e))
        }
    }
    // HttpResponse::Ok().json(user)
}

#[get("/api/v1/user/userbytoken")]
async fn get_user_by_token(
    // params: web::Query<i32>,
    db: web::Data<mysql_utils::DatabaseManager>,
    req: HttpRequest,
) -> impl Responder {
    //判断token是否有效

    let auth_header = match req.headers().get("Authorization") {
        Some(header) => match header.to_str() {
            Ok(str) => str,
            Err(_) => {
                return HttpResponse::Unauthorized().body("Invalid authorization header format")
            }
        },
        None => return HttpResponse::Unauthorized().body("Missing authorization header"),
    };

    if !auth_header.starts_with("Bearer ") {
        return HttpResponse::Unauthorized().body("Invalid token format");
    }

    let token_result: Result<&str, &str> = Ok(auth_header.split(" ").nth(1).unwrap_or(""));

    match token_result {
        Ok(auth_header) => {
            // params: web::Query<oauth2::AuthorizationCode>,
            let user = match db.get_user_by_token(auth_header).await {
                Ok(user) => user,
                Err(_) => {
                    return HttpResponse::NotFound()
                        .json("User not found for given token or token is invalid")
                }
            };

            return HttpResponse::Ok().json(user);
        }
        Err(_) => {
            // If token extraction or validation fails, return Unauthorized
            return HttpResponse::Unauthorized().body("Invalid or missing token");
        }
    }
}

async fn get_github_emails(access_token: &str) -> Result<Vec<EmailResponse>, Error> {
    // let client = reqwest::Client::new();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30)) // 添加超时
        .danger_accept_invalid_certs(true) // 仅开发环境使用！
        .build()
        .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;

    let response_text = client
        .get("https://api.github.com/user/emails")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "YOUR_APP_NAME")
        .send()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to fetch emails: {}", e)))?
        .text()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to parse response text: {}", e)))?;

    let response: Vec<EmailResponse> = serde_json::from_str(&response_text)
        .map_err(|e| ErrorInternalServerError(format!("Failed to parse email response: {}", e)))?;

    Ok(response)
}

async fn get_google_emails(access_token: &str) -> Result<String, Error> {
    let client = reqwest::Client::new();
    let response_text = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to fetch google user info: {}", e)))?
        .text()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to parse response text: {}", e)))?;

    let user_info: Value = serde_json::from_str(&response_text).map_err(|e| {
        ErrorInternalServerError(format!("Failed to parse user info response: {}", e))
    })?;

    match user_info.get("email") {
        Some(email) => Ok(email.as_str().unwrap_or("").to_string()),
        None => Err(ErrorInternalServerError("Email not found in response")),
    }
}

// #[post("/api/v1/user/logout")]
// async fn logout(req: HttpRequest, db: web::Data<mysql_utils::DatabaseManager>) -> impl Responder {
//     let auth_header = match req.headers().get("Authorization") {
//         Some(header) => match header.to_str() {
//             Ok(str) => str,
//             Err(_) => {
//                 return HttpResponse::Unauthorized().body("Invalid authorization header format")
//             }
//         },
//         None => return HttpResponse::Unauthorized().body("Missing authorization header"),
//     };

//     if !auth_header.starts_with("Bearer ") {
//         return HttpResponse::Unauthorized().body("Invalid token format");
//     }

//     let token = auth_header.split(" ").nth(1).unwrap_or("");

//     match db.invalidate_token(token).await {
//         Ok(_) => HttpResponse::Ok().json(json!({
//             "message": "Successfully logged out"
//         })),
//         Err(e) => HttpResponse::InternalServerError().body(format!("Failed to logout: {}", e)),
//     }
// }

#[derive(Deserialize)]
struct EmailResponse {
    email: String,
    primary: bool,
    verified: bool,
    visibility: Option<String>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = env_logger::try_init();

    // 初始化数据库
    let mysql_connect = dotenv::var("MYSQL_URL").unwrap();
    let _db_manager = mysql_utils::new(mysql_connect.as_str()).await.unwrap();

    let db_manager = web::Data::new(_db_manager);
    db_manager.create_table().await.unwrap();

    // kafka obj
    let kafka_obj = match kafka::KafkaHandler::new(
        env::var("BROKER").expect("broker must be set").as_str(),
        kafka::KAFKA_TOPIC,
    ) {
        Ok(handler) => handler,
        Err(e) => panic!("Failed to create Kafka handler: {}", e),
    };
    let kafka_handler = web::Data::new(kafka_obj);
    // Further initialization or setup if needed
    // For example, you might want to call some setup function on db_manager
    info!("starting web server...");
    println!("starting web server...");

    //github auth client
    let github_client = create_github_oauth_client();
    let github_auth_client = web::Data::new(github_client);
    //google auth client
    let google_client = create_google_oauth_client();
    let google_auth_client = web::Data::new(google_client);
    //email manager
    let email_manager = web::Data::new(EmailManager::new().await);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:8080")
            .allowed_origin("http://localhost:5173")
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::ACCEPT,
            ])
            .allowed_header(actix_web::http::header::CONTENT_TYPE)
            .max_age(3600);
        App::new()
            .wrap(cors)
            .wrap(Logger::default())
            .service(get_auth_url)
            .service(get_github_auth_url)
            .service(get_user)
            .service(send_email)
            .service(oauth_url)
            .service(oauth2callback)
            .service(email_login_register)
            .service(get_user_by_token)
            .service(get_user_by_email)
            .app_data(db_manager.clone())
            .app_data(kafka_handler.clone())
            .app_data(github_auth_client.clone())
            .app_data(google_auth_client.clone())
            .app_data(email_manager.clone())
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}
