use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::message::{MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message as LettreMessage, Tokio1Executor};
use std::sync::{Arc, Mutex};

use super::{EmailMessage, EmailProvider};
use crate::notification::config::SmtpConfig;

pub struct SmtpEmailProvider {
    config: SmtpConfig,
}

impl SmtpEmailProvider {
    pub fn new(config: SmtpConfig) -> Self {
        Self { config }
    }

    fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        let has_credentials = self.config.username.is_some() && self.config.password.is_some();
        tracing::debug!(
            host = %self.config.host,
            port = self.config.port,
            use_tls = self.config.use_tls,
            has_credentials = has_credentials,
            "Building SMTP transport"
        );

        let mut builder = if self.config.use_tls {
            if self.config.port == 465 {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)
                    .with_context(|| {
                        format!(
                            "Failed to configure SMTP TLS relay for {}:{}",
                            self.config.host, self.config.port
                        )
                    })?
                    .port(self.config.port)
            } else {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.host)
                    .with_context(|| {
                        format!(
                            "Failed to configure SMTP STARTTLS relay for {}:{}",
                            self.config.host, self.config.port
                        )
                    })?
                    .port(self.config.port)
            }
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.host)
                .port(self.config.port)
        };

        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            builder = builder.credentials(Credentials::new(username.clone(), password.clone()));
        }

        Ok(builder.build())
    }
}

#[async_trait]
impl EmailProvider for SmtpEmailProvider {
    async fn send(&self, message: EmailMessage) -> Result<()> {
        if message.to.is_empty() {
            tracing::warn!("Skipping SMTP send: no recipients specified in email message");
            return Ok(());
        }

        tracing::info!(
            host = %self.config.host,
            port = self.config.port,
            use_tls = self.config.use_tls,
            from = %message.from,
            recipient_count = message.to.len(),
            recipients = ?message.to,
            subject = %message.subject,
            "Connecting to SMTP server for notification email"
        );

        let transport = match self.build_transport() {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(
                    host = %self.config.host,
                    port = self.config.port,
                    use_tls = self.config.use_tls,
                    %e,
                    "Failed to build SMTP transport"
                );
                return Err(e);
            }
        };

        let mut sent_count = 0;
        let mut errors = Vec::new();

        for recipient in &message.to {
            tracing::info!(
                recipient = %recipient,
                subject = %message.subject,
                host = %self.config.host,
                port = self.config.port,
                "Sending notification email"
            );

            let parsed_from = match message.from.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    let err_msg = format!("Invalid sender address '{}': {e}", message.from);
                    tracing::error!(from = %message.from, %e, "Invalid sender address format");
                    return Err(anyhow!(err_msg));
                }
            };

            let parsed_to = match recipient.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    let err_msg = format!("Invalid recipient address '{recipient}': {e}");
                    tracing::error!(recipient = %recipient, %e, "Invalid recipient address format");
                    errors.push(err_msg);
                    continue;
                }
            };

            let email_builder = LettreMessage::builder()
                .from(parsed_from)
                .to(parsed_to)
                .subject(&message.subject);

            let email = if let Some(html) = &message.html_body {
                match email_builder.multipart(
                    MultiPart::alternative()
                        .singlepart(
                            SinglePart::builder()
                                .header(ContentType::TEXT_PLAIN)
                                .body(message.text_body.clone()),
                        )
                        .singlepart(
                            SinglePart::builder()
                                .header(ContentType::TEXT_HTML)
                                .body(html.clone()),
                        ),
                ) {
                    Ok(m) => m,
                    Err(e) => {
                        let err_msg =
                            format!("Failed to build multipart email for {recipient}: {e}");
                        tracing::error!(recipient = %recipient, %e, "Failed to build multipart email message");
                        errors.push(err_msg);
                        continue;
                    }
                }
            } else {
                match email_builder
                    .header(ContentType::TEXT_PLAIN)
                    .body(message.text_body.clone())
                {
                    Ok(m) => m,
                    Err(e) => {
                        let err_msg =
                            format!("Failed to build plain text email for {recipient}: {e}");
                        tracing::error!(recipient = %recipient, %e, "Failed to build plain text email message");
                        errors.push(err_msg);
                        continue;
                    }
                }
            };

            match transport.send(email).await {
                Ok(_) => {
                    sent_count += 1;
                    tracing::info!(
                        recipient = %recipient,
                        subject = %message.subject,
                        "Notification email sent successfully"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        recipient = %recipient,
                        host = %self.config.host,
                        port = self.config.port,
                        from = %message.from,
                        subject = %message.subject,
                        %e,
                        "SMTP server error while sending notification email"
                    );
                    errors.push(format!("Failed to send email to {recipient}: {e}"));
                }
            }
        }

        if !errors.is_empty() {
            let combined = errors.join("; ");
            tracing::error!(
                sent_count,
                total_recipients = message.to.len(),
                errors = %combined,
                "Encountered errors while sending notification emails"
            );
            return Err(anyhow!("SMTP send errors: {combined}"));
        }

        tracing::info!(
            sent_count,
            total_recipients = message.to.len(),
            subject = %message.subject,
            "All notification emails sent successfully"
        );
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct MockEmailProvider {
    pub sent_messages: Arc<Mutex<Vec<EmailMessage>>>,
}

impl MockEmailProvider {
    pub fn new() -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn messages(&self) -> Vec<EmailMessage> {
        self.sent_messages.lock().unwrap().clone()
    }
}

#[async_trait]
impl EmailProvider for MockEmailProvider {
    async fn send(&self, message: EmailMessage) -> Result<()> {
        self.sent_messages.lock().unwrap().push(message);
        Ok(())
    }
}
