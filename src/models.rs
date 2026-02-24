use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Deserialize)]
pub struct TransferRequest {
    pub name: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub content_type: String,
}

#[derive(Serialize, Deserialize)]
pub struct PendingTransfer {
    pub key: String,
    pub size: u64,
}

#[derive(Serialize)]
pub struct TransferResponse {
    pub ok: u8,
    pub id: String,
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    pub key: String,
}

// Internal representation for AES-GCM encryption
#[derive(Serialize, Deserialize, Clone)]
pub struct ImageResponsePayload {
    pub url: String,
    pub f: String,
    pub s: f64,
    pub t: i64,
    pub d: String,
    #[serde(rename = "P")]
    pub p: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c: Option<u8>,
}

// What the client actually receives
#[derive(Serialize)]
pub struct ObfuscatedResponse {
    pub ok: u8,
    pub payload: String,
}

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
    LargePayload(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(e) => write!(f, "Bad Request: {e}"),
            Self::NotFound(e) => write!(f, "Not Found: {e}"),
            Self::Internal(e) => write!(f, "Internal Error: {e}"),
            Self::LargePayload(e) => write!(f, "Payload Too Large: {e}"),
        }
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::LargePayload(_) => StatusCode::PAYLOAD_TOO_LARGE,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({
            "ok": 0,
            "e": self.to_string(),
        }))
    }
}
