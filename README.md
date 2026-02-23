# ImgDock — Rust Backend

High-performance image upload API built with Rust. Handles presigned URL generation for direct client-to-R2 uploads, with MongoDB for metadata and Redis for caching.

## Architecture

```
Client (Frontend)
    │
    ├─ POST /transfer ──────────► Rust Backend
    │                                 │
    │                                 ├─ Validates (image type, max size)
    │                                 ├─ Generates 6-char unique ID
    │                                 ├─ Creates presigned URL (5 min expiry)
    │                                 ├─ Stores pending state in Redis (5 min TTL)
    │                                 └─ Returns { id, uploadUrl, key }
    │
    ├─ PUT uploadUrl ───────────► Cloudflare R2 (Direct Upload, no proxy)
    │
    ├─ POST /transfer/{id}/done ► Rust Backend
    │                                 │
    │                                 ├─ Fetches pending from Redis
    │                                 ├─ Verifies file exists on R2 (HeadObject)
    │                                 ├─ Saves metadata to MongoDB
    │                                 ├─ Caches public URL in Redis (24h)
    │                                 └─ Returns { ok, id }
    │
    └─ GET /i/{id} ─────────────► Rust Backend
                                      │
                                      ├─ Checks Redis cache first
                                      ├─ Falls back to MongoDB
                                      └─ Returns { ok, url }
```

## Tech Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| **HTTP Server** | Actix-web 4 | Async, multi-threaded web framework |
| **Object Storage** | Cloudflare R2 (AWS SDK) | Image storage via S3-compatible API |
| **Database** | MongoDB 3 | Image metadata persistence |
| **Cache** | Redis (Fred 9) | Pending transfers + URL caching |
| **TLS** | Native TLS | Secure Redis/MongoDB connections |
| **Logging** | env_logger + Actix Logger | Request logging + structured logs |

## Project Structure

```
render-rust/
├── Cargo.toml              # Dependencies & release profile
├── .env                    # Environment variables (not committed)
├── README.md               # This file
└── src/
    ├── main.rs             # Entry point: connects services, starts server
    ├── config.rs           # Loads all config from environment variables
    ├── handlers.rs         # API endpoint handlers (4 routes)
    └── models.rs           # Request/Response structs + error handling
```

### File Details

| File | Lines | What It Does |
|------|-------|-------------|
| `main.rs` | ~109 | Initializes S3/MongoDB/Redis connections, sets up CORS & logging middleware, binds HTTP server |
| `config.rs` | ~39 | Reads 9 environment variables into a `Config` struct with defaults for `PORT` and `MAX_SIZE_MB` |
| `handlers.rs` | ~252 | Contains all 4 endpoint handlers + helper functions (`gen_id`, `date_folder`, `unix_timestamp`) |
| `models.rs` | ~100 | Request/Response models + centralized `AppError` enum with automatic HTTP status mapping |

## Prerequisites

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustc --version   # Verify: needs edition 2021+
```

### 2. Install OpenSSL (required for TLS)

**Ubuntu / Debian:**
```bash
sudo apt-get update
sudo apt-get install -y libssl-dev pkg-config
```

**macOS:**
```bash
brew install openssl
```

**Arch Linux:**
```bash
sudo pacman -S openssl pkg-config
```

### 3. External Services Required

| Service | What You Need | Where To Get It |
|---------|--------------|----------------|
| **Cloudflare R2** | Bucket + API keys | [Cloudflare Dashboard](https://dash.cloudflare.com/) → R2 |
| **MongoDB** | Connection URI | [MongoDB Atlas](https://cloud.mongodb.com/) (free tier available) |
| **Redis** | Connection URL with TLS | [Upstash](https://upstash.com/) (free tier available) |

## Setup

### 1. Clone & Enter Directory

```bash
cd render-rust
```

### 2. Create `.env` File

Create a `.env` file in the `render-rust/` directory:

```bash
# ─── Cloudflare R2 Storage ───
R2_ENDPOINT=https://<account-id>.r2.cloudflarestorage.com
R2_BUCKET=imgdock
R2_ACCESS_KEY=your_r2_access_key
R2_SECRET_KEY=your_r2_secret_key
R2_PUBLIC_DOMAIN=https://pub-xxxx.r2.dev

# ─── MongoDB ───
MONGO_URI=mongodb+srv://user:password@cluster.mongodb.net/imgdock

# ─── Redis (TLS required for Upstash) ───
REDIS_URL=rediss://default:token@your-redis.upstash.io:6379

# ─── Server ───
PORT=3000

# ─── Upload Limit (in MB, default: 99) ───
MAX_SIZE_MB=99
```

### 3. Build & Run

**Development:**
```bash
cargo build           # Compile (debug mode)
cargo run             # Run server
```

**Production:**
```bash
cargo build --release               # Optimized binary (LTO enabled)
./target/release/imgdock             # Run production binary
```

**With custom log level:**
```bash
RUST_LOG=debug cargo run             # Verbose logging
RUST_LOG=warn cargo run              # Warnings only
```

## API Reference

### `POST /transfer` — Create Upload Transfer

Creates a presigned URL for direct upload to R2.

**Request:**
```json
{
  "name": "photo.jpg",
  "size": 2048576,
  "type": "image/jpeg"
}
```

**Success Response (200):**
```json
{
  "ok": 1,
  "id": "aB3xY9",
  "uploadUrl": "https://r2.cloudflarestorage.com/imgdock/20260222/photo.jpg?X-Amz-...",
  "key": "20260222/photo.jpg"
}
```

**Errors:**
| Code | Condition |
|------|-----------|
| 400 | Missing/empty name, non-image type |
| 413 | File exceeds `MAX_SIZE_MB` |
| 500 | Redis/S3 connection error |

---

### `POST /transfer/{id}/done` — Complete Transfer

Verifies the file was uploaded to R2 and saves metadata.

**Request:** No body needed. Just the `id` in the URL.

**Success Response (200):**
```json
{
  "ok": 1,
  "id": "aB3xY9"
}
```

**Errors:**
| Code | Condition |
|------|-----------|
| 400 | File not found on R2 (not uploaded) |
| 404 | Transfer ID expired or not found |
| 500 | MongoDB/Redis error |

---

### `GET /i/{id}` — Get Image URL

Returns the public R2 URL for an image. Uses Redis cache (24h TTL).

**Success Response (200):**
```json
{
  "ok": 1,
  "url": "https://pub-xxxx.r2.dev/20260222/photo.jpg",
  "c": 1
}
```

> `"c": 1` means the response came from Redis cache. Absent if fetched from MongoDB.

**Errors:**
| Code | Condition |
|------|-----------|
| 404 | Image ID not found |
| 500 | MongoDB/Redis error |

---

### `GET /health` — Health Check

**Response (200):**
```json
{
  "ok": 1
}
```

## Environment Variables Reference

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `R2_ENDPOINT` | ✅ | — | Cloudflare R2 S3-compatible endpoint URL |
| `R2_BUCKET` | ✅ | — | R2 bucket name |
| `R2_ACCESS_KEY` | ✅ | — | R2 API access key |
| `R2_SECRET_KEY` | ✅ | — | R2 API secret key |
| `R2_PUBLIC_DOMAIN` | ✅ | — | Public URL prefix for R2 bucket |
| `MONGO_URI` | ✅ | — | MongoDB connection string |
| `REDIS_URL` | ✅ | — | Redis connection URL (supports `rediss://` for TLS) |
| `PORT` | ❌ | `3000` | HTTP server port |
| `MAX_SIZE_MB` | ❌ | `99` | Maximum upload file size in MB |
| `RUST_LOG` | ❌ | `info` | Log level (`debug`, `info`, `warn`, `error`) |

## MongoDB Document Schema

Collection: `imgdock.i`

```json
{
  "_id": "aB3xY9",
  "f": "20260222/photo.jpg",
  "s": 1.95,
  "t": 1740240000,
  "d": "",
  "P": ""
}
```

| Field | Type | Description |
|-------|------|-------------|
| `_id` | String | 6-char unique image ID |
| `f` | String | R2 file path (YYYYMMDD/filename) |
| `s` | Float | File size in MB (rounded to 2 decimal) |
| `t` | Int64 | Upload unix timestamp (seconds) |
| `d` | String | Google Drive file ID (set by sync script) |
| `P` | String | Reserved field |

## Deploy to Render

1. Create a new **Web Service** on [Render](https://render.com)
2. Set **Build Command**: `cargo build --release`
3. Set **Start Command**: `./target/release/imgdock`
4. Add all required environment variables from the table above
5. Deploy 🚀

## Performance

| Feature | Detail |
|---------|--------|
| **Lock-free state** | No `RwLock`/`Mutex` — Redis (fred) and MongoDB drivers are internally thread-safe |
| **Connection pooling** | Both MongoDB and Redis maintain internal connection pools |
| **Zero-alloc date** | Date formatting uses `std::time` + Hinnant algorithm (no `chrono` dependency) |
| **Optimized binary** | Release profile: `lto=true`, `codegen-units=1`, `strip=true` |
| **Presigned uploads** | Files upload directly to R2 from the client — server never proxies data |
| **Redis caching** | Image URLs cached for 24h, reducing MongoDB reads |
