use actix_web::{
    get, http::header, middleware::Logger, post, web, App, Error, HttpRequest, HttpResponse,
    HttpServer, Responder,
};

// #[derive(Debug, FromRow, Serialize, Deserialize)]
// pub struct UserManager {
//     pub DBhandler: web::Data<DatabaseManager>,
//     pub email: String,
// }

// impl UserManager {
//     //new one
//     pub fn new(db: web::Data<DatabaseManager>, email: String) -> Self {
//         Self {
//             DBhandler: db,
//             email,
//         }
//     }

//     //generate_token
//     pub fn generate_token(&self) -> String {
//         let mut rng = thread_rng();
//         let token: String = std::iter::repeat(())
//             .map(|()| rng.sample(Alphanumeric))
//             .take(32)
//             .collect();
//         token
//     }

//     //get user by email
//     pub async fn generate_tokens_by_email(&self) -> Result<String, sqlx::Error> {
//         //get user by email if not exist return info

//         match self.dbhandler.get_user_by_email(&self.email).await {
//             Ok(user) => Ok(user),
//             Ok(None) => {
//                 let topai_token = generate_token();
//                 //insert user
//                 self.dbhandler
//                     .create_update_user(&self.email, &topai_token)
//                     .await?;
//                 //generate token
//                 let token = generate_token();
//                 //update token
//                 self.dbhandler
//                     .update_token_by_email(&self.email, &token)
//                     .await?;
//                 Ok(user)
//             }
//             Err(e) => Err(e),
//         }
//         let user = self.get_user_by_email(&self.email).await?;
//     }
// }

// impl UserManager {
//     pub async fn get_user_by_email(&self, email: &str) -> Result<UserManager, sqlx::Error> {
//         let user = sqlx::query_as!(
//             UserManager,
//             r#"
//             SELECT * FROM user_account WHERE email = $1
//             "#,
//             email
//         )
//         .fetch_one(&self.pool)
//         .await?;
//         Ok(user)
//     }
// }

pub fn generate_store_token(
    db: web::Data<DatabaseManager>,
    email: String,
) -> Result<String, sqlx::Error> {
    match db.create_update_user(email).await {
        Ok((user_account, new_user)) => {
            println!("start create_update_user {:?}", new_user);

            //sync to vouch
            if !params.ex_info.voucher_code.is_empty() {
                // TODO: Upload voucher code to specified URL
                // Implementation needed to post voucher_code to endpoint
                if let Err(e) =
                    exchange_voucher_code(user_account.user_id, params.ex_info.voucher_code).await
                {
                    eprintln!("Failed to exchange voucher code: {}", e);
                }
            }
            if new_user {
                //sendmsg to kafka

                let mut map: HashMap<String, Value> = HashMap::new();
                map.insert("user_id".to_string(), json!(user_account.user_id));
                map.insert("type".to_string(), json!("user"));
                kafka_handler.send_message("user", map).await.unwrap();
            }

            return HttpResponse::Ok().json(json!({
                "topai_token": user_account.token
            }));
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .body(format!("Failed to create user: {}", e))
        }
    };
}
