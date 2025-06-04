//sqlite
use sqlx::sqlite::SqlitePool;
use tonic;

use actix_cors::Cors;
use actix_session::{Session, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::error::ErrorInternalServerError;
use actix_web::{
    get, middleware::Logger, post, web, App, Error, HttpRequest, HttpResponse, HttpServer,
    Responder,
};

use reqwest::Error as ReqwestError;

use oauth2::TokenResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use dotenv;
use log::*;
use serde_json::json;
use std::env;
// email part

// use reqwest::{self, StatusCode};
use std::collections::HashMap;

use oauth2::{
    basic::{BasicClient, BasicErrorResponseType},
    AuthUrl, ClientId, ClientSecret, CsrfToken, HttpRequest as OAuth2HttpRequest,
    HttpResponse as OAuth2HttpResponse, RedirectUrl, RequestTokenError, Scope,
    StandardErrorResponse, TokenUrl,
};

use UserAgent::communication::aws_utils::{EmailManReq, EmailManager};
// use DatabaseManager
use UserAgent::database::mysql_utils::{self};

use UserAgent::database::sqlite_utils::{self};
//mq
use UserAgent::mq::kafka;

use UserAgent::routes::api;

use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode as httpStatusCode};
use reqwest;
use std::time::Duration;

//proxy
use tonic::transport::Server;
use UserAgent::llmproxy::llm_proxy_server::LlmProxyServer;

// const MarketingServerUrl: &str = "topai-marketing-server.demo-ray.svc.cluster.local/api/v1/voucher/exchange:80";
const MARKETING_SERVER_URL: &str = "159.135.196.73:32756";

//redirect url

const REDIRECT_URL: &str = "http://localhost:5173";

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
        Ok((user_account, new_user, last_login_old)) => {
            println!("start create_update_user {:?}", new_user);

            //sync to vouch
            // if !voucher_code.is_empty() {
            //     // TODO: Upload voucher code to specified URL
            //     // Implementation needed to post voucher_code to endpoint
            //     if let Err(e) = exchange_voucher_code(user_account.user_id, voucher_code).await {
            //         eprintln!("Failed to exchange voucher code: {}", e);
            //     }
            // }

            let current_time = chrono::Utc::now().date_naive();
            let last_login = user_account
                .last_login_time
                .map(|dt| dt.date_naive())
                .unwrap_or(current_time);

            let last_login_old_date = last_login_old.date_naive();

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

            println!(
                "last_login_old_date: {}, last_login: {}, registration_time: {}",
                last_login_old_date.to_string(),
                last_login.to_string(),
                user_account.registration_time.date_naive().to_string(),
            );

            //last_login_old
            if last_login == user_account.registration_time.date_naive()
                || last_login > last_login_old_date
            {
                let mut map: HashMap<String, Value> = HashMap::new();
                map.insert("user_id".to_string(), json!(user_account.user_id));
                map.insert("type".to_string(), json!("daily_online"));
                match kafka_handler.send_message("credits", map).await {
                    Ok(_) => {
                        println!("send kafka credits daily_online success");
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
        env::var("MARKETING_SERVER_URL").unwrap_or_else(|_| MARKETING_SERVER_URL.to_string());

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
// 在 Actix-web 中，当有多个相同类型的 `web::Data<T>` 时，依赖注入是按照参数声明的顺序进行匹配的。这就是为什么建议使用自定义类型来区分不同的客户端。

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = env_logger::try_init();
    dotenv::dotenv().ok();
    // 初始化mysql数据库
    let mysql_connect = std::env::var("MYSQL_URL").unwrap();
    info!("mysql connect url is: {}", mysql_connect);

    let _db_manager = mysql_utils::new(mysql_connect.as_str()).await.unwrap();
    let db_manager = web::Data::new(_db_manager);
    db_manager.create_table().await.unwrap();

    //初始化sqlite3
    let sqlite_connect = dotenv::var("SQLITE_URL")
        .unwrap_or_else(|_| "sqlite:///Users/harryma/sqlite.db".to_string());
    let pool = SqlitePool::connect(sqlite_connect.as_str()).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    sqlite_utils::init_db(&mut *conn).await.unwrap();

    //gprc
    // 1. 启动 gRPC 服务（用 tokio::spawn）
    tokio::spawn(async {
        let addr = "0.0.0.0:50051".parse().unwrap();
        println!("gRPC LLM Proxy listening on {}", addr);

        Server::builder()
            .add_service(LlmProxyServer::new(LLMProxyService))
            .serve(addr)
            .await
            .expect("gRPC server failed");
    });

    // // kafka obj
    // let kafka_obj = match kafka::KafkaHandler::new(
    //     env::var("BROKER").expect("broker must be set").as_str(),
    //     kafka::KAFKA_TOPIC,
    // ) {
    //     Ok(handler) => handler,
    //     Err(e) => panic!("Failed to create Kafka handler: {}", e),
    // };
    // let kafka_handler = web::Data::new(kafka_obj);
    // Further initialization or setup if needed
    // For example, you might want to call some setup function on db_manager
    info!("starting web server...");
    println!("starting web server...");

    // let github_client = web::Data::new(GithubClient(create_github_oauth_client()));
    // let google_client = web::Data::new(GoogleClient(create_google_oauth_client()));

    let google_login_man = web::Data::new(api::GoogleLoginMan::new());
    let github_login_man = web::Data::new(api::GithubLoginMan::new());
    let aliyunsms_login_man = web::Data::new(api::SmsMan::new());
    //email manager
    let email_manager = web::Data::new(EmailManager::new().await);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:5173")
            .allowed_origin("http://localhost:3000")
            .allowed_origin("http://localhost:8000")
            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::ACCEPT,
                actix_web::http::header::CONTENT_TYPE,
            ])
            .allowed_header(actix_web::http::header::CONTENT_TYPE)
            .supports_credentials()
            .max_age(3600);

        // let cors = Cors::default()
        //     // 允许多个源
        //     .allowed_origin("http://localhost:5173")
        //     .allowed_origin("http://127.0.0.1:5173")
        //     // 允许所有的请求头
        //     .allow_any_header()
        //     // 允许所有的请求方法
        //     .allow_any_method()
        //     // 允许特定的路
        //     // 允许携带认证信息
        //     .supports_credentials();
        // let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(Logger::default())
            //api部分
            .service(
                web::scope("/api/v1/login")
                    .route(
                        "/google_request",
                        web::get().to(move |data: web::Data<api::GoogleLoginMan>| async move {
                            data.get_ref().generate_oauth_url()
                        }),
                    )
                    .route(
                        "/google_callback",
                        web::post().to(move |data: web::Data<api::GoogleLoginMan>, params: web::Json<api::Auth2CallbackParameters> | async move {
                            data.get_ref().google_callback(params).await
                        }),
                    )
                    .route(
                        "/github_request",
                        web::get().to(move |data: web::Data<api::GithubLoginMan>| async move {
                            data.get_ref().generate_oauth_url()
                        }),
                    )
                    .route(
                        "/github_callback",
                        web::post().to(move |data: web::Data<api::GithubLoginMan>, params: web::Json<api::Auth2CallbackParameters> | async move {
                            data.get_ref().github_callback(params).await
                        }),
                    ).route(
                        "aliyun_sms",
                        web::post().to(move |data: web::Data<api::SmsMan>, params: web::Json<api::SmsManReq> | async move {
                            data.get_ref().send_phone_code(params.phone.as_str(),params.data.as_str()).await
                        }),
                    ).route(
                        "email_code",
                        web::post().to(move |data: web::Data<EmailManager>, params: web::Json<EmailManReq> | async move {
                            data.get_ref().send_email_to(params.email.as_str(),params.pin_code.as_str()).await
                        }),
                    )
            )
            .app_data(google_login_man.clone())
            .app_data(github_login_man.clone())
            .app_data(db_manager.clone())
            .app_data(web::Data::new(pool.clone()))
            .app_data(aliyunsms_login_man.clone())
            // .app_data(kafka_handler.clone())
            // .app_data(github_client.clone())
            // .app_data(google_client.clone())
            .app_data(email_manager.clone())
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}

pub struct LLMProxyService;

#[tonic::async_trait]
impl UserAgent::llmproxy::llm_proxy_server::LlmProxy for LLMProxyService {
    async fn chat_completion(
        &self,
        request: tonic::Request<UserAgent::llmproxy::ChatCompletionRequest>,
    ) -> Result<tonic::Response<UserAgent::llmproxy::ChatCompletionResponse>, tonic::Status> {
        // 你的实现代码
        todo!()
    }
}
