use actix_cors::Cors;
use actix_session::{Session, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::{
    get, middleware::Logger, post, web, App, Error, HttpResponse, HttpServer, Responder,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use aws_sdk_sesv2::types::EmailContent;
use dotenv;
use log::*;
use serde_json::json;
use std::env;
// email part
use aws_config;
use aws_sdk_sesv2 as sesv2;
use aws_sdk_sesv2::types::Destination;
use aws_sdk_sesv2::types::Template;
use aws_sdk_sesv2::Client;
use std::collections::HashMap;

// use aws_types::region::Region;
// use aws_sdk_ses::config::{Builder, Config};
///
/// let config = aws_sdk_ses::Config::builder()
///     .region(Region::new("us-east-1"))
///     .build();
///
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, RedirectUrl, Scope, TokenUrl,
};

// use DatabaseManager
use UserAgent::database::mysql_utils;

//mq
use UserAgent::mq::kafka;

// 配置常量
const CLIENT_SECRETS_FILE: &str = "client_secret.json";
const SCOPES: &[&str] = &["https://www.googleapis.com/auth/drive.metadata.readonly"];
const API_SERVICE_NAME: &str = "drive";
const API_VERSION: &str = "v2";

#[derive(Serialize, Deserialize)]
struct UserInfo {
    name: String,
    email: String,
}

//github oauth client
fn create_github_oauth_client() -> BasicClient {
    let github_client_id = ClientId::new(
        env::var("GITHUB_CLIENT_ID").expect("Missing GITHUB_CLIENT_ID environment variable."),
    );
    let github_client_secret = ClientSecret::new(
        env::var("GITHUB_CLIENT_SECRET")
            .expect("Missing GITHUB_CLIENT_SECRET environment variable."),
    );
    // let auth_url = AuthUrl::new("
    // let client = BasicClient::new(
    //         ClientId::new("your_client_id".to_string()),
    //         Some(ClientSecret::new("your_client_secret".to_string())),
    //         AuthUrl::new("https://github.com/login/oauth/authorize".to_string())?,
    //         Some(TokenUrl::new("https://github.com/login/oauth/access_token".to_string())?)
    //     )
    //     .set_redirect_uri(RedirectUrl::new("http://localhost:8000/callback".to_string())?);

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
        RedirectUrl::new("http://localhost:8080/callback".to_string())
            .expect("Invalid redirect URL"),
    )
}

//google oauth client
fn create_oauth_client() -> BasicClient {
    let google_client_id = ClientId::new(
        env::var("GOOGLE_CLIENT_ID").expect("Missing GOOGLE_CLIENT_ID environment variable."),
    );
    let google_client_secret = ClientSecret::new(
        env::var("GOOGLE_CLIENT_SECRET")
            .expect("Missing GOOGLE_CLIENT_SECRET environment variable."),
    );

    //proxy ip
    let proxy_url = env::var("PROXY_URL").expect("Missing PROXY_URL environment variable.");
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
        RedirectUrl::new("http://localhost:8080/callback".to_string())
            .expect("Invalid redirect URL"),
    )
}

// #[derive(Clone)]
// struct AppState {
//     oauth_client: BasicClient,
//     http_client: HttpClient,
// }

// #[derive(Serialize, Deserialize)]
// struct Credentials {
//     token: String,
//     refresh_token: Option<String>,
//     token_uri: String,
//     client_id: String,
//     client_secret: String,
//     scopes: Vec<String>,
// }

#[get("/api/oauth2callback")]
async fn oauth2callback(
    oauth_client: web::Data<BasicClient>,
    params: web::Query<oauth2::AuthorizationCode>,
    session: Session,
) -> Result<HttpResponse, Error> {
    println!("oauth2callback");
    println!("{:?}", params);
    // let token = oauth_client
    //     .exchange_code(oauth2::AuthorizationCode::new(params.code.clone()))
    //     .request_async(oauth2::reqwest::async_http_client)
    //     .await
    //     .map_err(|_| HttpResponse::InternalServerError())?;

    Ok(HttpResponse::Ok().body("Everything went well!"))

    // // 验证CSRF令牌
    // let csrf_token: String = session.get("csrf_token").unwrap().unwrap();
    // if csrf_token != query.get("state").unwrap() {
    //     return HttpResponse::BadRequest().body("Invalid CSRF token");
    // }

    // // 交换授权码获取访问令牌
    // let token_result = data.oauth_client
    //     .exchange_code(oauth2::AuthorizationCode::new(query.get("code").unwrap().to_string()))
    //     .request_async(oauth2::reqwest::async_http_client)
    //     .await;

    // match token_result {
    //     Ok(token) => {
    //         // 存储凭证
    //         let credentials = Credentials {
    //             token: token.access_token().secr·et().to_string(),
    //             refresh_token: token.refresh_token().map(|rt| rt.secret().to_string()),
    //             token_uri: data.oauth_client.token_url().unwrap().to_string(),
    //             client_id: data.oauth_client.client_id().unwrap().to_string(),
    //             client_secret: data.oauth_client.client_secret().unwrap().to_string(),
    //             scopes: token.scopes().unwrap().iter().map(|s| s.to_string()).collect(),
    //         };
    //         session.set("credentials", credentials).unwrap();

    //         HttpResponse::Found()
    //             .header("Location", "/test")
    //             .finish()
    //     },
    //     Err(e) => HttpResponse::InternalServerError().body(format!("Error: {}", e)),
    // }
}

#[derive(Deserialize, Debug)]
struct EmailParameters {
    email: String,
}

#[post("/api/v1/auth/email/send_pincode")]
async fn send_email(
    web::Json(params): web::Json<EmailParameters>,
    db: web::Data<mysql_utils::DatabaseManager>,
) -> impl Responder {
    println!(".{:?}", params.email);

    let glID = dotenv::var("GOOGLE_CLIENT_ID").unwrap();
    println!(".{:?}", glID);
    // 设置 AWS 客户端
    let config = aws_config::load_from_env().await;
    let client = Client::new(&config);

    // todo need to generate code and store to mysql
    let template_data = json!({
        "name": params.email,
        "pin_code": "12345678"
    });

    let template_data_str = serde_json::to_string(&template_data).unwrap();

    // 创建电子邮件内容，使用模板
    let email_content = EmailContent::builder()
        .template(
            Template::builder()
                .template_name("TopAIPinCodeNotificationTest1") // 使用在 SES 控制台中创建的模板名称
                .template_data(template_data_str)
                .build(),
        )
        .build();

    // 设置发件人和收件人
    let sender = "support@topnetwork.ai";
    let recipient = "max1015070108@gmail.com";

    // 发送邮件
    let reponses = client
        .send_email()
        .from_email_address(sender)
        .destination(Destination::builder().to_addresses(recipient).build())
        .content(email_content)
        .send()
        .await;

    println!("Email sent successfully {:?}", reponses);

    // store pincode to mysql
    // 1. email + created time
    //TODO db.user_pincode
    HttpResponse::Ok().json(json!({
        "message": "PINCODE sent to email"
    }))
}

#[derive(Deserialize)]
struct PinCodeVerification {
    email: String,
    pin_code: String,
}

/**
* verifying pincode
**/
#[post("/api/v1/auth/email/login_register")]
async fn verify_pincode(
    web::Json(params): web::Json<PinCodeVerification>,
    db: web::Data<mysql_utils::DatabaseManager>,
    // kafka_handler: web::Data<kafka::KafkaHandler>,
) -> impl Responder {
    // For demonstration purposes, we assume the correct pin code is "12345678"
    let correct_pin_code = "12345678";

    // get pincode from mysql and check if expired
    // db.get_pincode_by_email
    if params.pin_code != correct_pin_code {
        return HttpResponse::Unauthorized().body("Invalid pin code");
    }

    match db.create_update_user(&params.email).await {
        Ok((user_account, new_user)) => {
            println!("start create_update_user{:?}", new_user);
            // if new_user {
            //     //sendmsg to kafka

            //     let mut map: HashMap<String, Value> = HashMap::new();
            //     map.insert("user_id".to_string(), json!(user_account.user_id));
            //     map.insert("type".to_string(), json!("user"));
            //     kafka_handler.send_message("user", map);
            // }

            return HttpResponse::Ok().json(json!({
                "user_id": user_account.user_id,
                "topai_token": user_account.token
            }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .body(format!("Failed to create user: {}", e))
        }
    };

    //sendmsg to kafka

    HttpResponse::Ok().body("Pin code verified successfully")
}

#[get("/api/v1/oauth/init/{provider}")]
async fn oauth_url(path: web::Path<(String,)>) -> impl Responder {
    let provider = path.into_inner().0;
    match provider.as_str() {
        "google" => {
            println!("match google......");
            let client = create_oauth_client();
            let (auth_url, _csrf_token) = client
                .authorize_url(CsrfToken::new_random)
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
            let (auth_url, _csrf_token) = client
                .authorize_url(CsrfToken::new_random)
                .add_scope(Scope::new("user:email".to_string())) //.set_pkce_challenge(pkce_challenge)
                .url();

            println!("Open this URL in your browser:\n{}", auth_url);

            return HttpResponse::Ok().body(auth_url.to_string());
        }
        _ => return HttpResponse::BadRequest().body("Unsupported provider"),
    };

    return HttpResponse::Ok().body("Unsupported error".to_string());
}

#[get("/api/v1/auth_google_url")]
async fn get_auth_url() -> impl Responder {
    let client = create_oauth_client();
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

#[get("/api/v1/github_auth_url")]
async fn get_github_auth_url() -> impl Responder {
    let client = create_github_oauth_client();
    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("user:email".to_string())) //.set_pkce_challenge(pkce_challenge)
        .url();

    println!("Open this URL in your browser:\n{}", auth_url);

    HttpResponse::Ok().body(auth_url.to_string())
}

#[get("/api/user")]
async fn get_user() -> impl Responder {
    // 这里应该实现从session中获取用户信息的逻辑
    // 为了演示，我们返回一个模拟的用户
    let user = UserInfo {
        name: "John Doe".to_string(),
        email: "johndoe@example.com".to_string(),
    };
    HttpResponse::Ok().json(user)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = env_logger::try_init();

    // 初始化数据库
    let mysql_connect = dotenv::var("MYSQL_URL").unwrap();
    let _db_manager = mysql_utils::new(mysql_connect.as_str()).await.unwrap();

    let db_manager = web::Data::new(_db_manager);
    db_manager.create_table().await.unwrap();

    //kafka obj
    let kafka_handler = web::Data::new(kafka::KafkaHandler::new(
        env::var("BROKER").expect("broker must be set").as_str(),
        kafka::KAFKA_TOPIC,
    ));
    // Further initialization or setup if needed
    // For example, you might want to call some setup function on db_manager
    info!("starting web server...");
    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:8080")
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
            .service(verify_pincode)
            .app_data(db_manager.clone())
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}
