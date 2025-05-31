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

use UserAgent::communication::aws_utils::EmailManager;
// use DatabaseManager
use UserAgent::database::mysql_utils::{self};
//mq
use UserAgent::mq::kafka;

use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode as httpStatusCode};
use reqwest;
use std::time::Duration;

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
            if !voucher_code.is_empty() {
                // TODO: Upload voucher code to specified URL
                // Implementation needed to post voucher_code to endpoint
                if let Err(e) = exchange_voucher_code(user_account.user_id, voucher_code).await {
                    eprintln!("Failed to exchange voucher code: {}", e);
                }
            }

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

    let redirect_uri = env::var("REDIRECT_URL").unwrap_or_else(|_| REDIRECT_URL.to_string());
    BasicClient::new(
        github_client_id,
        Some(github_client_secret),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(
        //
        RedirectUrl::new(format!("{}/auth/callback/github", redirect_uri))
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

    let redirect_uri = env::var("REDIRECT_URL").unwrap_or_else(|_| REDIRECT_URL.to_string());

    BasicClient::new(
        google_client_id,
        Some(google_client_secret),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/auth/callback/google", redirect_uri))
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
    web::Json(params): web::Json<Auth2CallbackParameters>,
    db: web::Data<mysql_utils::DatabaseManager>,
    google_client: web::Data<GoogleClient>, // 只会匹配 GoogleClient
    github_client: web::Data<GithubClient>,
    kafka_handler: web::Data<kafka::KafkaHandler>,
) -> Result<HttpResponse, Error> {
    println!("oauth2callback");
    println!("{:?}", params.code);

    let voucher_code = params.state;
    println!("voucher_code: {}", voucher_code);

    let provider = path.into_inner().0;
    match provider.as_str() {
        "google" => {
            let proxy_endpoint = env::var("PROXY_HOST").unwrap_or("".to_string());

            println!("proxy_endpoint: {:?}, start create client", proxy_endpoint);
            // Assume `client` is your reqwest client configured with the proxy
            let client = reqwest::Client::builder()
                .proxy(reqwest::Proxy::http(proxy_endpoint.as_str()).unwrap()) // Set HTTP proxy
                .proxy(reqwest::Proxy::https(proxy_endpoint.as_str()).unwrap()) // Set HTTPS proxy
                .timeout(Duration::from_secs(30))
                .danger_accept_invalid_certs(true)
                .build()
                .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;

            println!("start exchange code");
            let token = match google_client
                .0
                .exchange_code(oauth2::AuthorizationCode::new(params.code))
                .request_async(move |request: oauth2::HttpRequest| {
                    println!("start....");
                    let client = client.clone(); // Clone client to move into async block

                    async move {
                        // Use 'client' inside the async block
                        let method = request.method;
                        let url = request.url;
                        let headers = request.headers;
                        let body = request.body;

                        println!("body is {:?}", body);

                        // Build and execute reqwest request with our custom client
                        let mut req_builder = client.request(
                            reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap(),
                            url,
                        );

                        println!("start proxy request");

                        // Add headers
                        for (k, v) in headers.iter() {
                            req_builder = req_builder.header(k.as_str(), v.as_bytes());
                        }

                        req_builder = req_builder.body(body);

                        // Send the request
                        let resp = req_builder
                            .send()
                            .await
                            .map_err(|e| {
                                oauth2::RequestTokenError::<
                                    reqwest::Error,
                                    oauth2::StandardErrorResponse<
                                        oauth2::basic::BasicErrorResponseType,
                                    >,
                                >::Request(e)
                            })
                            .unwrap();

                        // Extract response components

                        let status = httpStatusCode::from_u16(resp.status().as_u16()).unwrap();
                        let headers = resp
                            .headers()
                            .iter()
                            .map(|(k, v)| {
                                (
                                    actix_web::http::header::HeaderName::from_bytes(
                                        <http::HeaderName as AsRef<[u8]>>::as_ref(k),
                                    )
                                    .unwrap(),
                                    actix_web::http::header::HeaderValue::from_bytes(v.as_bytes())
                                        .unwrap(),
                                )
                            })
                            .collect::<actix_web::http::header::HeaderMap>();
                        let bytes = resp
                            .bytes()
                            .await
                            .map_err(|e| {
                                oauth2::RequestTokenError::<
                                    reqwest::Error,
                                    oauth2::StandardErrorResponse<
                                        oauth2::basic::BasicErrorResponseType,
                                    >,
                                >::Request(e)
                            })
                            .unwrap();

                        Ok::<oauth2::HttpResponse, Error>(OAuth2HttpResponse {
                            status_code: actix_web::http::StatusCode::from_u16(status.as_u16())
                                .unwrap(),
                            headers: headers.into(),
                            body: bytes.into(), //Vec::new(),
                        })
                        // Ok().body("".to_string())
                    }
                })
                .await
            {
                Ok(token) => token,
                Err(e) => {
                    println!("Failed to exchange code: {:?}", e);
                    return Ok(actix_web::HttpResponse::InternalServerError().json(json!({
                        "error": format!("Failed to exchange code: {:?}", e)
                    })));
                }
            };

            //could get more info from token
            let user_info_str = get_google_emails(token.access_token().secret().as_str())
                .await
                .unwrap();

            println!("user_info: {:?}", user_info_str);

            let user_info: Value = serde_json::from_str(&user_info_str).unwrap();
            let primary_email = user_info["email"].as_str().unwrap().to_string();

            match generate_tokens(primary_email.to_string(), voucher_code, db, kafka_handler).await
            {
                Ok(topai_token) => {
                    println!("start create update .......");
                    return Ok(HttpResponse::Ok().json(json!({
                        "topai_token": topai_token
                    })));
                }
                Err(e) => {
                    println!("Error creating or updating user: {}", e);
                    return Ok(HttpResponse::InternalServerError().json(json!({
                        "error": e.to_string()
                    })));
                }
            }

            // return Ok(HttpResponse::Ok().json(json!({
            //     "topai_token": token.access_token().secret(),
            // })));
        }
        "github" => {
            println!("github start exchange codes {:?}", params.code);
            let token = match github_client
                .0
                .exchange_code(oauth2::AuthorizationCode::new(params.code))
                .request_async(oauth2::reqwest::async_http_client)
                .await
            {
                Ok(token) => token,
                Err(e) => {
                    println!("Failed to exchange code: {:?}", e);
                    return Ok(HttpResponse::InternalServerError().json(json!({
                        "error": format!("Failed to exchange code: {:?}", e)
                    })));
                }
            };

            let user_info = get_github_emails(token.access_token().secret().as_str())
                .await
                .unwrap();

            //todo 如果需要获取更多信息，可以在这里获取user_info.1
            let primary_email = user_info
                .0
                .iter()
                .find(|e| e.primary)
                .map(|e| &e.email)
                .unwrap_or(&user_info.0[0].email);
            println!("Got github user primary email: {}", primary_email);

            // db.create_update_user(primary_email.as_str()).await;
            match generate_tokens(primary_email.to_string(), voucher_code, db, kafka_handler).await
            {
                Ok(topai_token) => {
                    println!("start create update .......");
                    return Ok(HttpResponse::Ok().json(json!({
                        "topai_token": topai_token
                    })));
                }
                Err(e) => {
                    println!("Error creating or updating user: {}", e);
                    return Ok(HttpResponse::InternalServerError().json(json!({
                        "error": e.to_string()
                    })));
                }
            }

            // Ok(HttpResponse::Ok().json(token))
        }
        _ => Ok(HttpResponse::Ok().json(json!({
            "message": "CURRENT PROVIDER NOT SUPPORTED"
        }))),
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
    let new_pincode = match get_or_generate_pincode(params.email.as_str(), db).await {
        Ok(pincode) => pincode,
        Err(e) => {
            error!("Error generating pincode: {}", e);
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("Failed to generate_pincode: {}", e) }));
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
            HttpResponse::InternalServerError().json(json!({
                "error": format!("Failed to send_pincode: {}", e)
            }))
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
    let correct_pin_code = match db.get_pincode_by_email(params.email.as_str()).await {
        Ok(pincode) => match pincode {
            Some(user_pincode) => {
                let now = chrono::Utc::now();
                let duration = now.signed_duration_since(user_pincode.created_at);

                if duration.num_minutes() >= 10 {
                    return HttpResponse::Ok().json(json!({
                        "error": "Pincode expired"
                    }));
                }
                user_pincode.pincode
            }
            None => {
                return HttpResponse::Ok().json(json!({
                    "error": "No pincode found for this email"
                }));
            }
        },
        Err(e) => {
            println!("Error getting pincode: {}", e);
            return HttpResponse::InternalServerError().json(json!({
                "error": format!("Failed to get pincode: {}", e)
            }));
        }
    };

    // if params.pin_code != pin_code {
    //     return HttpResponse::Unauthorized().body("mismatch Invalid pin code");
    // }
    if params.pin_code != correct_pin_code {
        return HttpResponse::Ok().json(json!({
            "error": "mismatch Invalid pin code"
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
            return HttpResponse::InternalServerError().json(json!({
                "error": format!("Failed to create user: {}", e)
            }));
        }
    }
}

#[get("/api/v1/user/oauth/init/{provider}")]
async fn oauth_url_init(
    path: web::Path<(String,)>,
    web::Query(params): web::Query<HashMap<String, String>>,
    google_client: web::Data<GoogleClient>, // 只会匹配 GoogleClient
    github_client: web::Data<GithubClient>,
    // google_client: web::Data<BasicClient>,
) -> impl Responder {
    let voucher_code = params
        .get("voucher_code")
        .unwrap_or(&"".to_string())
        .to_string();
    println!("init voucher_code: {}", voucher_code);

    let provider = path.into_inner().0;
    match provider.as_str() {
        "google" => {
            println!("match google......");
            let client = create_google_oauth_client();
            let (auth_url, _csrf_token) = google_client
                .0
                // .authorize_url(CsrfToken::new_random)
                .authorize_url(move || CsrfToken::new(voucher_code.clone()))
                .add_scope(Scope::new(
                    "https://www.googleapis.com/auth/userinfo.email".to_string(),
                ))
                .add_scope(Scope::new(
                    "https://www.googleapis.com/auth/userinfo.profile".to_string(),
                ))
                .url();

            return HttpResponse::Ok().json(json!({
                "redirect_url": auth_url.to_string()
            }));

            // return HttpResponse::Ok().body("hello");
        }
        "github" => {
            println!("match github......");
            // let client = create_github_oauth_client();
            let csrf_state = voucher_code;
            let (auth_url, _csrf_token) = github_client
                .0
                .authorize_url(move || CsrfToken::new(csrf_state.clone()))
                .add_scope(Scope::new("user:email".to_string()))
                .add_scope(Scope::new("read:user".to_string())) //.set_pkce_challenge(pkce_challenge)
                .url();

            println!("Open this URL in your browser:\n{}", auth_url);

            // return HttpResponse::Ok().body(auth_url.to_string());
            return HttpResponse::Ok().json(json!({
                "redirect_url": auth_url.to_string()
            }));
        }
        _ => {
            return HttpResponse::Ok().json(json!({
                "error": "Unsupported provider"
            }))
        }
    };

    // return HttpResponse::Ok().body("Unsupported error".to_string());
}

#[get("/api/v1/user/userinfo")]
async fn get_user(db: web::Data<mysql_utils::DatabaseManager>, req: HttpRequest) -> impl Responder {
    // 这里应该实现从session中获取用户信息的逻辑
    // 为了演示，我们返回一个模拟的用户
    // let user = UserInfo {
    //     name: "John Doe".to_string(),
    //     email: "johndoe@example.com".to_string(),
    // };
    //
    let user = UserInfo {
        name: "Test User".to_string(),
        email: "test@example.com".to_string(),
    };

    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = auth_str.trim_start_matches("Bearer ");
                match db.get_user_by_token(token).await {
                    Ok(user_info) => {
                        return HttpResponse::Ok().json(user_info);
                    }
                    Err(_) => {
                        return HttpResponse::Unauthorized().json(json!({
                            "error": "Invalid token"
                        }));
                    }
                }
            }
        }
    }

    return HttpResponse::Unauthorized().json(json!({
        "error": "Missing Authorization header"
    }));

    // HttpResponse::Ok().json(user)
}

#[get("/api/v1/user/getuserbyemail")]
async fn get_user_by_email(
    web::Query(params): web::Query<HashMap<String, String>>,
    db: web::Data<mysql_utils::DatabaseManager>,
) -> impl Responder {
    let email = match params.get("email") {
        Some(email) => email,
        None => {
            return HttpResponse::Ok().json(json!({
                "error": "Missing email parameter"
            }));
        }
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
                return HttpResponse::Unauthorized().json(json!({
                    "error": "Invalid authorization header format"
                }))
            }
        },
        None => {
            return HttpResponse::Unauthorized().json(json!({
                "error": "Missing authorization header"
            }))
        }
    };

    if !auth_header.starts_with("Bearer ") {
        return HttpResponse::Unauthorized().json(json!({
            "error": "Invalid token format"
        }));
    }

    let token_result: Result<&str, &str> = Ok(auth_header.split(" ").nth(1).unwrap_or(""));

    match token_result {
        Ok(auth_header) => {
            // params: web::Query<oauth2::AuthorizationCode>,
            let user = match db.get_user_by_token(auth_header).await {
                Ok(user) => user,
                Err(_) => {
                    return HttpResponse::NotFound().json(json!({
                        "error": "User not found for given token or token is invalid"
                    }))
                }
            };

            return HttpResponse::Ok().json(user);
        }
        Err(_) => {
            // If token extraction or validation fails, return Unauthorized
            return HttpResponse::Unauthorized().json(json!({
                "error": "Invalid or missing token"
            }));
        }
    }
}

async fn get_github_emails(access_token: &str) -> Result<(Vec<EmailResponse>, Value), Error> {
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

    let response_text_profile = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "YOUR_APP_NAME")
        .send()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to fetch emails: {}", e)))?
        .text()
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to parse response text: {}", e)))?;

    println!("profile detail: {}", response_text_profile);
    let response_profile: Value = serde_json::from_str(&response_text_profile).unwrap();
    println!("response_profile detail: {}", response_profile);

    let response_email: Vec<EmailResponse> = serde_json::from_str(&response_text)
        .map_err(|e| ErrorInternalServerError(format!("Failed to parse email response: {}", e)))?;

    Ok((response_email, response_profile))
}

async fn get_google_emails(access_token: &str) -> Result<String, Error> {
    let proxy_endpoint = env::var("PROXY_HOST").unwrap_or("".to_string());

    if proxy_endpoint.is_empty() {
        return Err(ErrorInternalServerError("Proxy host not set".to_string()));
    }

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(proxy_endpoint.as_str()).unwrap()) // 设置HTTP代理
        .proxy(reqwest::Proxy::https(proxy_endpoint.as_str()).unwrap()) // 设置HTTPS代理
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;

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

    println!("user_info: {:?}", user_info);
    Ok(response_text)
}

#[post("/api/v1/user/logout")]
async fn logout(req: HttpRequest, db: web::Data<mysql_utils::DatabaseManager>) -> impl Responder {
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

    let token = auth_header.split(" ").nth(1).unwrap_or("");

    match db.invalidate_token(token).await {
        Ok(_) => HttpResponse::Ok().json(json!({
            "message": "Successfully logged out"
        })),
        Err(e) => HttpResponse::Ok().json(json!({
            "error": format!("Failed to logout: {}", e),
        })),
    }
}

#[derive(Deserialize)]
struct EmailResponse {
    email: String,
    primary: bool,
    verified: bool,
    visibility: Option<String>,
}

// 在 Actix-web 中，当有多个相同类型的 `web::Data<T>` 时，依赖注入是按照参数声明的顺序进行匹配的。这就是为什么建议使用自定义类型来区分不同的客户端。
#[derive(Debug)]
struct GoogleClient(BasicClient);

#[derive(Debug)]
struct GithubClient(BasicClient);

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

    let github_client = web::Data::new(GithubClient(create_github_oauth_client()));
    let google_client = web::Data::new(GoogleClient(create_google_oauth_client()));

    //email manager
    let email_manager = web::Data::new(EmailManager::new().await);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin("http://localhost:5173")
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
            // .service(get_auth_url)
            // .service(get_github_auth_url)
            .service(get_user)
            .service(send_email)
            .service(oauth_url_init)
            .service(oauth2callback)
            .service(email_login_register)
            .service(get_user_by_token)
            .service(get_user_by_email)
            .app_data(db_manager.clone())
            .app_data(kafka_handler.clone())
            .app_data(github_client.clone())
            .app_data(google_client.clone())
            .app_data(email_manager.clone())
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}
