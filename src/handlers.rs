use actix_web::{web, HttpResponse};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::Client as S3Client;
use fred::prelude::*;
use mongodb::Collection;
use rand::rngs::OsRng;
use rand::Rng;
use serde_json::json;
use std::time::Duration;

use crate::config::Config;
use crate::models::{AppError, ImageResponsePayload, ObfuscatedResponse, PendingTransfer, TransferRequest, TransferResponse};

pub struct AppState {
    pub config: Config,
    pub s3: S3Client,
    pub db: Collection<mongodb::bson::Document>,
    pub redis: RedisClient,
}

/// Returns (YYYYMMDD date folder, unix timestamp seconds)
fn now_parts() -> (String, i64) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let days = secs / 86400;
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    #[allow(clippy::cast_possible_wrap)]
    (format!("{y}{m:02}{d:02}"), secs as i64)
}

fn gen_id() -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

/// Encrypts JSON payload using AES-256-GCM. Returns hex-encoded "iv + ciphertext + `auth_tag`"
fn encrypt_payload(json: &str, key: &[u8; 32]) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    rand::RngCore::fill_bytes(&mut OsRng, &mut nonce_bytes);

    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), json.as_bytes())
        .map_err(|_| AppError::Internal("Encryption failure".into()))?;

    // Prepend 12-byte IV/nonce to ciphertext
    let mut final_payload = nonce_bytes.to_vec();
    final_payload.extend_from_slice(&ciphertext);

    Ok(hex::encode(final_payload))
}

// POST /transfer
pub async fn create_transfer(
    state: web::Data<AppState>,
    body: web::Json<TransferRequest>,
) -> Result<HttpResponse, AppError> {
    if body.name.is_empty() {
        return Err(AppError::BadRequest("Name cannot be empty".into()));
    }

    let content_type = body.content_type.to_lowercase();
    if !state.config.allowed_formats.contains(&content_type)
        && !state.config.allowed_formats.contains(&"*".to_string())
    {
        return Err(AppError::BadRequest(format!(
            "Unsupported file format. Allowed: {}",
            state.config.allowed_formats.join(", ")
        )));
    }
    if body.size > state.config.max_size {
        return Err(AppError::LargePayload(format!(
            "Max {}MB",
            state.config.max_size_mb
        )));
    }

    let id = gen_id();
    let (date, _) = now_parts();
    let key = format!("{date}/{}", body.name);

    log::info!("Transfer: {id} → {key}");

    let presign_config = PresigningConfig::builder()
        .expires_in(Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let upload_url = state
        .s3
        .put_object()
        .bucket(&state.config.r2_bucket)
        .key(&key)
        .content_type(&body.content_type)
        .presigned(presign_config)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .uri()
        .to_string();

    let pending = PendingTransfer {
        key: key.clone(),
        size: body.size,
    };

    let pending_json =
        serde_json::to_string(&pending).map_err(|e| AppError::Internal(e.to_string()))?;

    state
        .redis
        .set::<(), _, _>(
            format!("pending:{id}"),
            &pending_json,
            Some(Expiration::EX(300)),
            None,
            false,
        )
        .await
        .map_err(|e| AppError::Internal(format!("Redis: {e}")))?;

    Ok(HttpResponse::Ok().json(TransferResponse {
        ok: 1,
        id,
        upload_url,
        key,
    }))
}

// POST /transfer/{id}/done
pub async fn complete_transfer(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    let redis_key = format!("pending:{id}");

    let pending_json: Option<String> = state
        .redis
        .get(&redis_key)
        .await
        .map_err(|e| AppError::Internal(format!("Redis: {e}")))?;

    let pending: PendingTransfer = serde_json::from_str(
        &pending_json.ok_or_else(|| AppError::NotFound("Transfer expired or not found".into()))?,
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    state
        .s3
        .head_object()
        .bucket(&state.config.r2_bucket)
        .key(&pending.key)
        .send()
        .await
        .map_err(|_| AppError::BadRequest("File not uploaded to storage".into()))?;

    log::info!("Verified: {id}");

    let (_, ts) = now_parts();
    let f = pending.key;

    // Convert to MB and round to 2 decimals using safe f64 conversion scaling
    #[allow(clippy::cast_precision_loss)]
    let s_mb = pending.size as f64 / 1_048_576.0;
    let s = (s_mb * 100.0).round() / 100.0;

    let url = format!(
        "{}/{}",
        state.config.r2_public_domain,
        urlencoding::encode(&f)
    );

    state
        .db
        .insert_one(mongodb::bson::doc! {
            "_id": &id,
            "f": &f,
            "s": s,
            "t": ts,
            "d": "",
            "P": "",
        })
        .await
        .map_err(|e| AppError::Internal(format!("MongoDB: {e}")))?;

    log::info!("Saved: {id}");

    // Cache internal payload JSON (without cache indicator yet)
    let internal_payload = ImageResponsePayload {
        url,
        f,
        s,
        t: ts,
        d: String::new(),
        p: String::new(),
        c: None,
    };

    let _: Result<(), _> = state.redis.del(&redis_key).await;

    if let Ok(json) = serde_json::to_string(&internal_payload) {
        let _: Result<(), _> = state
            .redis
            .set(
                format!("i:{id}"),
                &json,
                Some(Expiration::EX(86400)),
                None,
                false,
            )
            .await;
    }

    Ok(HttpResponse::Ok().json(json!({ "ok": 1, "id": id })))
}

// GET /i/{id}
#[allow(clippy::many_single_char_names)]
pub async fn get_image(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    let cache_key = format!("i:{id}");

    // Check Redis cache (stores internal payload JSON)
    if let Some(cached_json) = state
        .redis
        .get::<Option<String>, _>(&cache_key)
        .await
        .unwrap_or(None)
    {
        if let Ok(mut payload_obj) = serde_json::from_str::<ImageResponsePayload>(&cached_json) {
            payload_obj.c = Some(1); // Set cache flag to true
            let final_json = serde_json::to_string(&payload_obj).unwrap();
            let encrypted_hex = encrypt_payload(&final_json, &state.config.encryption_key)?;

            return Ok(HttpResponse::Ok().json(ObfuscatedResponse {
                ok: 1,
                payload: encrypted_hex,
            }));
        }
    }

    let doc = state
        .db
        .find_one(mongodb::bson::doc! { "_id": &id })
        .await
        .map_err(|e| AppError::Internal(format!("MongoDB: {e}")))?
        .ok_or_else(|| AppError::NotFound("Image not found".into()))?;

    let f = doc.get_str("f").unwrap_or("").to_string();
    let s = doc.get_f64("s").unwrap_or(0.0);
    let t = doc.get_i64("t").unwrap_or(0);
    let d = doc.get_str("d").unwrap_or("").to_string();
    let p = doc.get_str("P").unwrap_or("").to_string();

    let url = format!(
        "{}/{}",
        state.config.r2_public_domain,
        urlencoding::encode(&f)
    );

    let payload_obj = ImageResponsePayload {
        url,
        f,
        s,
        t,
        d,
        p,
        c: None,
    };

    // Cache internal payload JSON (24h)
    if let Ok(json) = serde_json::to_string(&payload_obj) {
        let _: Result<(), _> = state
            .redis
            .set(
                &cache_key,
                &json,
                Some(Expiration::EX(86400)),
                None,
                false,
            )
            .await;
    }

    let final_json = serde_json::to_string(&payload_obj).unwrap();
    let encrypted_hex = encrypt_payload(&final_json, &state.config.encryption_key)?;

    Ok(HttpResponse::Ok().json(ObfuscatedResponse {
        ok: 1,
        payload: encrypted_hex,
    }))
}

// GET /health
pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(json!({ "ok": 1 }))
}
