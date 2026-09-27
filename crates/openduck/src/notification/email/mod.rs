pub mod smtp;
pub mod template;

use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub text_body: String,
    pub html_body: Option<String>,
}

#[async_trait]
pub trait EmailProvider: Send + Sync {
    async fn send(&self, message: EmailMessage) -> Result<()>;
}
