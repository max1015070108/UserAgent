use anyhow::anyhow;
use anyhow::Error;
use aws_config;
use aws_sdk_sesv2::operation::create_email_template::CreateEmailTemplateError;
use aws_sdk_sesv2::types::EmailTemplateContent;
use aws_sdk_sesv2::{
    types::{Destination, EmailContent, Template},
    Client,
};
use env_logger::Target::Stdout;
use serde_json::json;
use std::io::Write;
pub struct EmailManager {
    pub file_path: std::path::PathBuf,
    client: Client,
}

impl EmailManager {
    pub async fn new() -> Self {
        Self {
            file_path: std::path::PathBuf::new(),
            client: Client::new(&aws_config::load_from_env().await),
        }
    }

    pub async fn createTemplate(&mut self) -> Result<(), Error> {
        let template_html = std::fs::read_to_string(&self.file_path)
            .unwrap_or_else(|_| "../../resources/topai_pin_notification.html".to_string());

        let template_content = EmailTemplateContent::builder()
            .subject("TopAIPin Code Notification")
            .html(template_html)
            .build();

        match self
            .client
            .create_email_template()
            .template_name("TopAIPinCodeNotificationTest1")
            .template_content(template_content)
            .send()
            .await
        {
            Ok(_) => {
                println!("Email template created successfully.");
                return Ok(());
            }
            Err(e) => match e.into_service_error() {
                CreateEmailTemplateError::AlreadyExistsException(_) => {
                    println!("Email template already exists, skipping creation.");
                }
                e => return Err(anyhow!("Error creating email template: {}", e)),
            },
        }
        Ok(())
    }

    pub async fn send_email_to(&self, email: &str, pincode: &str) -> Result<(), Error> {
        let template_data = json!({
            "name": email,
            "pin_code": pincode,
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
        // let recipient = "max1015070108@gmail.com";

        match self
            .client
            .send_email()
            .from_email_address(sender)
            .destination(Destination::builder().to_addresses(email).build())
            .content(email_content)
            .send()
            .await
        {
            Ok(response) => {
                println!("Email sent successfully {:?}", response);
                Ok(())
            }
            Err(e) => {
                println!("Error sending email: {:?}", e);
                return Err(e.into());
            }
        }

        // store pincode to mysql
        // 1. email + created time
        // //TODO db.user_pincode
        // HttpResponse::Ok().json(json!({
        //     "message": "PINCODE sent to email"
        // }))
    }
}
