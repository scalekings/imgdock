use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use std::fmt;

// ============ Request Models ============

#[derive(Deserialize)]
pub struct TransferRequest {
    pub name: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub content_type: String,
}

// ============ Redis Pending Data ============

#[derive(Serialize, Deserialize)]
pub struct PendingTransfer {
    pub key: String,
    pub name: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub content_type: String,
}

// ============ Response Models ============

#[derive(Serialize)]
pub struct TransferResponse {
    pub ok: u8,
    pub id: String,
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct CompleteResponse {
    pub ok: u8,
    pub id: String,
}

#[derive(Serialize)]
pub struct ImageResponse {
    pub ok: u8,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c: Option<u8>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub ok: u8,
}

// ============ Error Handling ============

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
            AppError::BadRequest(e) => write!(f, "Bad Request: {}", e),
            AppError::NotFound(e) => write!(f, "Not Found: {}", e),
            AppError::Internal(e) => write!(f, "Internal Error: {}", e),
            AppError::LargePayload(e) => write!(f, "Payload Too Large: {}", e),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    ok: u8,
    e: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::LargePayload(_) => StatusCode::PAYLOAD_TOO_LARGE,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ErrorBody {
            ok: 0,
            e: self.to_string(),
        })
    }
}
