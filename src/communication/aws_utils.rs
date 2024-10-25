use anyhow::anyhow;
use anyhow::Error;
use aws_config;
use aws_sdk_sesv2::operation::create_email_template::CreateEmailTemplateError;
use aws_sdk_sesv2::types::EmailTemplateContent;
use aws_sdk_sesv2::Client;
use env_logger::Target::Stdout;
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

    // fn update(&mut self, updated_data: String) {
    //     self.data = updated_data;
    // }

    // fn send(&self) {
    //     println!("Sending data: {}", self.data);
    // }
}
