// src/api.rs

// use UserAgent::communication::sms;
use crate::communication::smscom;
use crate::database::sqlite_utils as db;
use actix_session::Session;
use actix_web::{
    cookie::Cookie, error::ErrorInternalServerError, get, post, web, Error, HttpRequest,
    HttpResponse, Responder,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
// use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, SqliteConnection, SqlitePool};

use oauth2::{
    basic::{BasicClient, BasicErrorResponseType},
    AuthUrl, ClientId, ClientSecret, CsrfToken, HttpRequest as OAuth2HttpRequest,
    HttpResponse as OAuth2HttpResponse, RedirectUrl, RequestTokenError, Scope,
    StandardErrorResponse, TokenUrl,
};

use dotenv;
use log::*;
use serde_json::json;
use std::env;

use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode as httpStatusCode};
use reqwest;

//database
use crate::database::mysql_utils;

//config
use crate::config::constant;

use oauth2::TokenResponse;
use serde_json::Value;

//sms import
use sms::aliyun::Aliyun;

#[derive(Deserialize)]
pub struct SendCodeRequest {
    phone: String,
}
#[post("/send_code")]
async fn send_code(data: web::Data<SqlitePool>, req: web::Json<SendCodeRequest>) -> impl Responder {
    let code = smscom::generate_code();
    smscom::save_code(&req.phone, &code);

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
    if smscom::verify_code(&req.phone, &req.code) {
        let exp = (Utc::now() + Duration::hours(2)).timestamp() as usize;
        let claims = Claims {
            sub: req.phone.clone(),
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(constant::JWT_SECRET),
        )
        .unwrap();
        HttpResponse::Ok().json(serde_json::json!({ "token": token }))
    } else {
        HttpResponse::Unauthorized().body("验证码错误")
    }
}

#[derive(Deserialize, Debug)]
pub struct Auth2CallbackParameters {
    pub code: String,
    pub state: String,
}

// create google login handler
#[derive(Clone)]
pub struct GoogleLoginMan {
    // pub google_client_id: String,
    // pub google_client_secret: String,
    pub google_client: BasicClient,
}

//constant URL

impl GoogleLoginMan {
    pub fn new() -> Self {
        dotenv::dotenv().ok();
        let google_client_id = env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID not set");
        let google_client_secret =
            env::var("GOOGLE_CLIENT_SECRET").expect("GOOGLE_CLIENT_SECRET not set");

        GoogleLoginMan {
            // google_client_id: google_client_id,
            // google_client_secret: google_client_secret,
            google_client: create_google_oauth_client(google_client_id, google_client_secret),
        }
    }

    // step 2 create oauth url
    pub fn generate_oauth_url(&self) -> HttpResponse {
        // let client = self.google_client;
        let state = format!("google_{}", uuid::Uuid::new_v4());

        let (authorize_url, _csrf_state) = self
            .google_client
            .authorize_url(|| CsrfToken::new(state.clone()))
            .add_scope(Scope::new(
                "https://www.googleapis.com/auth/userinfo.profile".to_string(),
            ))
            .add_scope(Scope::new(
                "https://www.googleapis.com/auth/userinfo.email".to_string(),
            ))
            .url();

        // authorize_url.to_string()
        // Responder
        //
        HttpResponse::Ok().json(serde_json::json!({ "url": authorize_url.to_string() }))
    }

    // step 3 callback google url

    pub async fn google_callback(
        &self,
        web::Json(params): web::Json<Auth2CallbackParameters>,
    ) -> Result<HttpResponse, Error> {
        let proxy_endpoint = env::var("PROXY_HOST").unwrap_or("".to_string());

        println!("proxy_endpoint: {:?}, start create client", proxy_endpoint);
        // Assume `client` is your reqwest client configured with the proxy
        // let client = reqwest::Client::builder()
        // .proxy(reqwest::Proxy::http(proxy_endpoint.as_str()).unwrap()) // Set HTTP proxy
        // .proxy(reqwest::Proxy::https(proxy_endpoint.as_str()).unwrap()) // Set HTTPS proxy
        // .danger_accept_invalid_certs(true)
        // .build()
        // .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;

        let mut client_builder = reqwest::Client::builder();
        if !proxy_endpoint.is_empty() {
            client_builder = client_builder
                .proxy(reqwest::Proxy::http(&proxy_endpoint).unwrap())
                .proxy(reqwest::Proxy::https(&proxy_endpoint).unwrap());
        }
        let client = client_builder
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;
        println!("start exchange code");

        match self
            .google_client
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
            Ok(token) => {
                println!("Token received successfully: {:?}", token);
                Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
            }
            Err(e) => {
                println!("Failed to exchange code: {:?}", e);
                if let oauth2::RequestTokenError::ServerResponse(err_response) = &e {
                    println!("Google error response: {:?}", err_response);
                }
                Ok(HttpResponse::InternalServerError().body(format!("Error: {}", e)))
                // Ok(HttpResponse::InternalServerError().body(format!("Error: {}", e)))
            }
        }
    }

    pub async fn get_google_emails(&self, access_token: &str) -> Result<String, Error> {
        let proxy_endpoint = env::var("PROXY_HOST").unwrap_or("".to_string());

        if proxy_endpoint.is_empty() {
            return Err(ErrorInternalServerError("Proxy host not set".to_string()));
        }

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(proxy_endpoint.as_str()).unwrap()) // 设置HTTP代理
            .proxy(reqwest::Proxy::https(proxy_endpoint.as_str()).unwrap()) // 设置HTTPS代理
            // .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;

        let response_text = client
            .get(constant::GOOGLE_USER_INFO_URL)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| {
                ErrorInternalServerError(format!("Failed to fetch google user info: {}", e))
            })?
            .text()
            .await
            .map_err(|e| {
                ErrorInternalServerError(format!("Failed to parse response text: {}", e))
            })?;

        let user_info: Value = serde_json::from_str(&response_text).map_err(|e| {
            ErrorInternalServerError(format!("Failed to parse user info response: {}", e))
        })?;

        println!("user_info: {:?}", user_info);
        Ok(response_text)
    }
}

//tool function
// step1 create client
pub fn create_google_oauth_client(
    google_client_id: String,
    google_client_secret: String,
) -> BasicClient {
    let google_client_id = ClientId::new(google_client_id);
    let google_client_secret = ClientSecret::new(google_client_secret);

    let auth_url = AuthUrl::new(constant::GOOGLE_AUTH.to_string())
        .expect("Invalid authorization endpoint URL");
    let token_url =
        TokenUrl::new(constant::GOOGLE_AUTH_TOKEN.to_string()).expect("Invalid token endpoint URL");

    let redirect_uri =
        env::var("REDIRECT_URL").unwrap_or_else(|_| constant::REDIRECT_URL.to_string());

    BasicClient::new(
        google_client_id,
        Some(google_client_secret),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/auth", redirect_uri)).expect("Invalid redirect URL"),
    )
}

//github
// create google login handler
#[derive(Clone)]
pub struct GithubLoginMan {
    // pub google_client_id: String,
    // pub google_client_secret: String,
    pub github_client: BasicClient,
}

impl GithubLoginMan {
    pub fn new() -> Self {
        dotenv::dotenv().ok();
        let github_client_id = env::var("GITHUB_CLIENT_ID").expect("GITHUB_CLIENT_ID not set");
        let github_client_secret =
            env::var("GITHUB_CLIENT_SECRET").expect("GITHUB_CLIENT_SECRET not set");

        GithubLoginMan {
            // google_client_id: google_client_id,
            // google_client_secret: google_client_secret,
            github_client: create_github_oauth_client(github_client_id, github_client_secret),
        }
    }

    // step 2 create oauth url
    pub fn generate_oauth_url(&self) -> HttpResponse {
        // let client = self.google_client;
        let state = format!("github_{}", uuid::Uuid::new_v4());
        let (authorize_url, _csrf_state) = self
            .github_client
            .authorize_url(move || CsrfToken::new(state.clone()))
            .add_scope(Scope::new("user:email".to_string()))
            .add_scope(Scope::new("read:user".to_string()))
            .url();

        // authorize_url.to_string()
        // Responder
        //
        HttpResponse::Ok().json(serde_json::json!({ "url": authorize_url.to_string() }))
    }

    // step 3 callback google url

    pub async fn github_callback(
        &self,
        web::Json(params): web::Json<Auth2CallbackParameters>,
    ) -> Result<HttpResponse, Error> {
        let proxy_endpoint = env::var("PROXY_HOST").unwrap_or("".to_string());

        println!("proxy_endpoint: {:?}, start create client", proxy_endpoint);
        // Assume `client` is your reqwest client configured with the proxy
        // let client = reqwest::Client::builder()
        // .proxy(reqwest::Proxy::http(proxy_endpoint.as_str()).unwrap()) // Set HTTP proxy
        // .proxy(reqwest::Proxy::https(proxy_endpoint.as_str()).unwrap()) // Set HTTPS proxy
        // .danger_accept_invalid_certs(true)
        // .build()
        // .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;

        let mut client_builder = reqwest::Client::builder();
        if !proxy_endpoint.is_empty() {
            client_builder = client_builder
                .proxy(reqwest::Proxy::http(&proxy_endpoint).unwrap())
                .proxy(reqwest::Proxy::https(&proxy_endpoint).unwrap());
        }
        let client = client_builder
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;
        println!("start exchange code");

        match self
            .github_client
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

                    println!("body is {:?}", String::from_utf8_lossy(&body));

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
            Ok(token) => {
                println!("Token received successfully: {:?}", token);
                Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
            }
            Err(e) => {
                println!("Failed to exchange code: {:?}", e);
                if let oauth2::RequestTokenError::ServerResponse(err_response) = &e {
                    println!("Google error response: {:?}", err_response);
                }
                Ok(HttpResponse::InternalServerError().body(format!("Error: {}", e)))
                // Ok(HttpResponse::InternalServerError().body(format!("Error: {}", e)))
            }
        }
    }

    pub async fn get_github_emails(&self, access_token: &str) -> Result<String, Error> {
        let proxy_endpoint = env::var("PROXY_HOST").unwrap_or("".to_string());

        if proxy_endpoint.is_empty() {
            return Err(ErrorInternalServerError("Proxy host not set".to_string()));
        }

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(proxy_endpoint.as_str()).unwrap()) // 设置HTTP代理
            .proxy(reqwest::Proxy::https(proxy_endpoint.as_str()).unwrap()) // 设置HTTPS代理
            // .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| ErrorInternalServerError(format!("Failed to build client: {}", e)))?;

        let response_text = client
            .get(constant::GITHUB_USER_INFO_URL)
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .map_err(|e| {
                ErrorInternalServerError(format!("Failed to fetch google user info: {}", e))
            })?
            .text()
            .await
            .map_err(|e| {
                ErrorInternalServerError(format!("Failed to parse response text: {}", e))
            })?;

        let user_info: Value = serde_json::from_str(&response_text).map_err(|e| {
            ErrorInternalServerError(format!("Failed to parse user info response: {}", e))
        })?;

        println!("user_info: {:?}", user_info);
        Ok(response_text)
    }
}

//tool function
// step1 create client
pub fn create_github_oauth_client(
    google_client_id: String,
    google_client_secret: String,
) -> BasicClient {
    let google_client_id = ClientId::new(google_client_id);
    let google_client_secret = ClientSecret::new(google_client_secret);

    let auth_url = AuthUrl::new(constant::GITHUB_AUTH.to_string())
        .expect("Invalid authorization endpoint URL");
    let token_url =
        TokenUrl::new(constant::GITHUB_AUTH_TOKEN.to_string()).expect("Invalid token endpoint URL");

    let redirect_uri =
        env::var("REDIRECT_URL").unwrap_or_else(|_| constant::REDIRECT_URL.to_string());

    BasicClient::new(
        google_client_id,
        Some(google_client_secret),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/auth", redirect_uri)).expect("Invalid redirect URL"),
    )
}

//sms
#[derive(Deserialize, Debug)]
pub struct SmsManReq {
    pub phone: String,
    pub data: String,
}

// #[derive(Clone)]
pub struct SmsMan {
    // pub aliyun_sms: Aliyun<'a>,
    pub sign_name: String,
    pub template_code: String,
    pub access_key_id: String,
    pub access_key_secret: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito;
    use std::env;

    #[tokio::test]
    async fn test_send_phone_code_real() {
        // 设置真实环境变量（或确保 shell 环境已设置）
        std::env::set_var("ALIYUN_ACCESS_KEY_ID", "你的真实key");
        std::env::set_var("ALIYUN_ACCESS_KEY_SECRET", "你的真实secret");
        std::env::set_var("ALIYUN_SMS_SIGN_NAME", "你的签名");
        std::env::set_var("ALIYUN_SMS_TEMPLATE_CODE", "你的模板");

        let sms_man = SmsMan::new();
        let result = sms_man.send_phone_code("15267067659").await;
        println!("阿里云返回: {:?}", result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_phone_code_error() {
        // Don't set required env vars to test error case
        env::remove_var("ALIYUN_ACCESS_KEY_ID");

        let sms_man = SmsMan::new();
        let result = sms_man.send_phone_code("13800138000").await;

        assert!(result.is_err());
    }
}

impl SmsMan {
    pub fn new() -> Self {
        dotenv::dotenv().ok();
        let access_key_id = env::var("ALIYUN_ACCESS_KEY_ID").expect("ALIYUN_ACCESS_KEY_ID not set");
        let access_key_secret =
            env::var("ALIYUN_ACCESS_KEY_SECRET").expect("ALIYUN_ACCESS_KEY_SECRET not set");
        let sign_name = env::var("SIGN_NAME").expect("SIGN_NAME not set");
        let temp_code = env::var("TEMP_CODE").expect("TEMP_CODE not set");

        SmsMan {
            access_key_id: access_key_id,
            access_key_secret: access_key_secret,
            sign_name: sign_name,
            template_code: temp_code,
        }
    }

    pub async fn send_phone_code(&self, phone: &str, data: &str) -> Result<String, Error> {
        let aliyun_sms_obj =
            Aliyun::new(self.access_key_id.as_str(), self.access_key_secret.as_str());
        let resp = aliyun_sms_obj
            .send_sms(
                phone,
                self.sign_name.as_str(),
                self.template_code.as_str(),
                data,
            )
            .await;

        // println("{:?}", resp);
        match resp {
            Ok(map) => Ok(map.get("Code").unwrap_or(&"".to_string()).to_string()),
            Err(e) => Err(ErrorInternalServerError(e.to_string())),
        }
        // Ok(response_text)
    }
}
