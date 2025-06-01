// src/sms.rs

use base64::{engine::general_purpose, Engine as _};
use chrono::{serde::ts_microseconds_option, Utc};
use hmac::{Hmac, Mac};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::Client;
use serde::Deserialize;
use sha1::Sha1;
use std::collections::BTreeMap;
use uuid::Uuid;

type HmacSha1 = Hmac<Sha1>;

#[derive(Deserialize, Debug)]
struct AliyunSmsResponse {
    Code: String,
    Message: String,
    RequestId: String,
    BizId: Option<String>,
}

/// 发送短信验证码
pub async fn send_sms_code(
    access_key_id: &str,
    access_key_secret: &str,
    sign_name: &str,
    template_code: &str,
    phone: &str,
    code: &str,
) -> Result<(), String> {
    // 1. 构造参数
    let mut params = BTreeMap::new();
    params.insert("AccessKeyId", access_key_id);
    params.insert("Action", "SendSms");
    params.insert("Format", "JSON");
    params.insert("PhoneNumbers", phone);
    params.insert("RegionId", "cn-hangzhou");
    params.insert("SignName", sign_name);
    params.insert("SignatureMethod", "HMAC-SHA1");
    let signature_nonce = Uuid::new_v4().to_string();
    params.insert("SignatureNonce", &signature_nonce);
    // params.insert("SignatureNonce", &Uuid::new_v4().to_string());
    params.insert("SignatureVersion", "1.0");
    params.insert("TemplateCode", template_code);
    let tmpp = &format!(r#"{{"code":"{}"}}"#, code);
    params.insert("TemplateParam", tmpp);

    let ts_microseconds_option = &Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    params.insert("Timestamp", ts_microseconds_option);
    params.insert("Version", "2017-05-25");

    // 2. 构造待签名字符串
    let mut query: Vec<String> = params
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                utf8_percent_encode(k, NON_ALPHANUMERIC),
                utf8_percent_encode(v, NON_ALPHANUMERIC)
            )
        })
        .collect();
    query.sort();
    let canonicalized_query_string = query.join("&");
    let string_to_sign = format!(
        "GET&%2F&{}",
        utf8_percent_encode(&canonicalized_query_string, NON_ALPHANUMERIC)
    );

    // 3. 计算签名
    let key = format!("{}&", access_key_secret);
    let mut mac = HmacSha1::new_from_slice(key.as_bytes()).unwrap();
    mac.update(string_to_sign.as_bytes());
    let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    let signature = utf8_percent_encode(&signature, NON_ALPHANUMERIC).to_string();

    // 4. 拼接最终URL
    let url = format!(
        "https://dysmsapi.aliyuncs.com/?Signature={}&{}",
        signature, canonicalized_query_string
    );

    // 5. 发送请求
    let client = Client::new();
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let res: AliyunSmsResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;

    if res.Code == "OK" {
        Ok(())
    } else {
        Err(format!("Aliyun SMS error: {} - {}", res.Code, res.Message))
    }
}

//======
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::sync::Mutex;

static CODE_STORE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn generate_code() -> String {
    let mut rng = thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

pub fn save_code(phone: &str, code: &str) {
    let mut store = CODE_STORE.lock().unwrap();
    store.insert(phone.to_string(), code.to_string());
}

pub fn verify_code(phone: &str, code: &str) -> bool {
    let mut store = CODE_STORE.lock().unwrap();
    if let Some(saved) = store.get(phone) {
        if saved == code {
            store.remove(phone);
            return true;
        }
    }
    false
}
