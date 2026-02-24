# ImgDock API Documentation

Frontend/UI developers ke liye complete API guide. Is document mein saare endpoints ka detail hai — request format, response format, error codes, aur JavaScript examples.

---

## 🌐 Base URL

```
http://localhost:3000
```

> Production mein apna deployed URL use karo (e.g. `https://your-app.onrender.com`)

---

## 🔐 CORS Configuration

Server **saare origins** se requests allow karta hai:

| Setting | Value |
|---------|-------|
| Origins | `*` (Any origin) |
| Methods | `GET`, `POST`, `PUT`, `OPTIONS` |
| Headers | `Content-Type` |
| Max Age | 3600 seconds (1 hour) |

---

## 📤 Upload Flow (3 Steps)

Image upload **3 step** mein hota hai:

```
Step 1: POST /transfer        → Presigned URL lo
Step 2: PUT  {uploadUrl}      → Direct R2 pe upload karo
Step 3: POST /transfer/{id}/done → Confirm karo
```

---

## 📋 API Endpoints

---

### 1️⃣ `POST /transfer` — Upload Shuru Karo

Presigned URL generate karta hai. Client isse use karke direct Cloudflare R2 pe upload karega.

#### Request

```http
POST /transfer
Content-Type: application/json
```

```json
{
  "name": "photo.jpg",
  "size": 2048576,
  "type": "image/jpeg"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | ✅ | File ka naam (extension ke saath) |
| `size` | number | ✅ | File size **bytes** mein |
| `type` | string | ✅ | MIME type — `image/` se shuru hona chahiye |

#### Validations

| Rule | Error |
|------|-------|
| `name` empty nahi hona chahiye | `400 Bad Request` |
| `type` `image/` se start hona chahiye | `400 Bad Request` |
| `size` ≤ MAX_SIZE_MB (default 99MB) | `413 Payload Too Large` |

#### Success Response — `200 OK`

```json
{
  "ok": 1,
  "id": "aB3xY9",
  "uploadUrl": "https://r2.cloudflarestorage.com/bucket/20260224/photo.jpg?X-Amz-Signature=...",
  "key": "20260224/photo.jpg"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `ok` | number | `1` = success |
| `id` | string | 6-character unique ID |
| `uploadUrl` | string | Presigned URL (5 min valid) — isse use karke file upload karo |
| `key` | string | R2 storage path (`YYYYMMDD/filename`) |

#### JavaScript Example

```javascript
async function startUpload(file) {
  const response = await fetch('http://localhost:3000/transfer', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name: file.name,
      size: file.size,
      type: file.type
    })
  });

  const data = await response.json();

  if (data.ok === 1) {
    console.log('Upload ID:', data.id);
    console.log('Upload URL:', data.uploadUrl);
    return data;
  } else {
    console.error('Error:', data.e);
  }
}
```

---

### 2️⃣ `PUT {uploadUrl}` — Direct R2 pe Upload Karo

Step 1 se jo `uploadUrl` mila, uspe directly file upload karo. **Yeh request R2 pe jaati hai, backend pe nahi.**

#### Request

```http
PUT {uploadUrl}
Content-Type: image/jpeg
Body: [raw file bytes]
```

#### JavaScript Example

```javascript
async function uploadToR2(uploadUrl, file) {
  const response = await fetch(uploadUrl, {
    method: 'PUT',
    headers: { 'Content-Type': file.type },
    body: file  // Direct File object pass karo
  });

  if (response.ok) {
    console.log('✅ File uploaded to R2!');
    return true;
  } else {
    console.error('❌ Upload failed:', response.status);
    return false;
  }
}
```

> ⚠️ **Note:** `uploadUrl` sirf **5 minute** ke liye valid hai. Uske baad expire ho jayega.

---

### 3️⃣ `POST /transfer/{id}/done` — Upload Confirm Karo

R2 pe upload hone ke baad, server ko batao ki upload complete hua. Server verify karega ki file sach mein R2 pe hai, phir MongoDB mein metadata save karega.

#### Request

```http
POST /transfer/{id}/done
```

> Koi request body nahi chahiye. Sirf URL mein `id` pass karo.

#### Success Response — `200 OK`

```json
{
  "ok": 1,
  "id": "aB3xY9"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `ok` | number | `1` = success |
| `id` | string | Image ka unique ID |

#### Error Responses

| Code | Condition | Response |
|------|-----------|----------|
| `404` | Transfer ID expire ho gaya ya nahi mila | `{"ok": 0, "e": "Not Found: Transfer expired or not found"}` |
| `400` | File R2 pe upload nahi hui | `{"ok": 0, "e": "Bad Request: File not uploaded to storage"}` |
| `500` | MongoDB/Redis error | `{"ok": 0, "e": "Internal Error: ..."}` |

#### JavaScript Example

```javascript
async function confirmUpload(id) {
  const response = await fetch(`http://localhost:3000/transfer/${id}/done`, {
    method: 'POST'
  });

  const data = await response.json();

  if (data.ok === 1) {
    console.log('✅ Upload confirmed! ID:', data.id);
    return data;
  } else {
    console.error('❌ Error:', data.e);
  }
}
```

---

### 4️⃣ `GET /i/{id}` — Image URL Lo

Image ka public URL retrieve karta hai. Pehle Redis cache check karta hai, nahi mila toh MongoDB se laata hai.

#### Request

```http
GET /i/{id}
```

#### Success Response — `200 OK`

**Cache se (Redis):**
```json
{
  "ok": 1,
  "url": "https://pub-xxxx.r2.dev/20260224/photo.jpg",
  "c": 1
}
```

**Database se (MongoDB):**
```json
{
  "ok": 1,
  "url": "https://pub-xxxx.r2.dev/20260224/photo.jpg"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `ok` | number | `1` = success |
| `url` | string | Image ka public URL |
| `c` | number (optional) | `1` = cache se aaya. Absent = MongoDB se aaya |

#### Error Responses

| Code | Condition | Response |
|------|-----------|----------|
| `404` | Image ID nahi mila | `{"ok": 0, "e": "Not Found: Image not found"}` |
| `500` | MongoDB/Redis error | `{"ok": 0, "e": "Internal Error: ..."}` |

#### JavaScript Example

```javascript
async function getImageUrl(id) {
  const response = await fetch(`http://localhost:3000/i/${id}`);
  const data = await response.json();

  if (data.ok === 1) {
    console.log('Image URL:', data.url);
    console.log('From cache:', data.c === 1 ? 'Yes' : 'No');
    return data.url;
  } else {
    console.error('❌ Error:', data.e);
  }
}

// HTML mein image dikhao
function showImage(id) {
  getImageUrl(id).then(url => {
    document.getElementById('preview').src = url;
  });
}
```

---

### 5️⃣ `GET /health` — Health Check

Server alive hai ya nahi, check karo.

#### Request

```http
GET /health
```

#### Response — `200 OK`

```json
{
  "ok": 1
}
```

#### JavaScript Example

```javascript
async function checkHealth() {
  try {
    const response = await fetch('http://localhost:3000/health');
    const data = await response.json();
    return data.ok === 1;
  } catch (e) {
    return false;
  }
}
```

---

## 🔄 Complete Upload Flow — Full JavaScript Example

Yeh ek complete function hai jo poora upload flow handle karta hai:

```javascript
async function uploadImage(file) {
  // Validate client-side
  if (!file.type.startsWith('image/')) {
    throw new Error('Sirf images upload kar sakte ho!');
  }

  // Step 1: Get presigned URL
  console.log('📋 Step 1: Getting upload URL...');
  const transferRes = await fetch('http://localhost:3000/transfer', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name: file.name,
      size: file.size,
      type: file.type
    })
  });

  const transfer = await transferRes.json();
  if (transfer.ok !== 1) throw new Error(transfer.e);

  // Step 2: Upload directly to R2
  console.log('⬆️ Step 2: Uploading to R2...');
  const uploadRes = await fetch(transfer.uploadUrl, {
    method: 'PUT',
    headers: { 'Content-Type': file.type },
    body: file
  });

  if (!uploadRes.ok) throw new Error('R2 upload failed');

  // Step 3: Confirm upload
  console.log('✅ Step 3: Confirming...');
  const confirmRes = await fetch(`http://localhost:3000/transfer/${transfer.id}/done`, {
    method: 'POST'
  });

  const confirm = await confirmRes.json();
  if (confirm.ok !== 1) throw new Error(confirm.e);

  console.log('🎉 Done! Image ID:', confirm.id);
  return confirm.id;
}

// Usage with HTML file input:
document.getElementById('fileInput').addEventListener('change', async (e) => {
  const file = e.target.files[0];
  if (!file) return;

  try {
    const imageId = await uploadImage(file);
    // Image dikhao
    const url = await getImageUrl(imageId);
    document.getElementById('preview').src = url;
  } catch (err) {
    alert('Error: ' + err.message);
  }
});
```

---

## ❌ Error Response Format

Saare errors ka format same hai:

```json
{
  "ok": 0,
  "e": "Error type: error message"
}
```

| HTTP Code | Error Type | Kab Aata Hai |
|-----------|-----------|--------------|
| `400` | Bad Request | Invalid input (empty name, non-image type, file not on R2) |
| `404` | Not Found | ID nahi mila (expired transfer, unknown image) |
| `413` | Payload Too Large | File size limit exceed (default: 99MB) |
| `500` | Internal Error | Server-side error (Redis/MongoDB/S3 issue) |

#### Error Handling Example

```javascript
async function apiCall(url, options = {}) {
  const response = await fetch(url, options);
  const data = await response.json();

  if (data.ok !== 1) {
    switch (response.status) {
      case 400: console.error('Invalid input:', data.e); break;
      case 404: console.error('Not found:', data.e); break;
      case 413: console.error('File too large:', data.e); break;
      case 500: console.error('Server error:', data.e); break;
    }
    throw new Error(data.e);
  }

  return data;
}
```

---

## 📊 Quick Reference Table

| Endpoint | Method | Body | Response |
|----------|--------|------|----------|
| `/transfer` | POST | `{name, size, type}` | `{ok, id, uploadUrl, key}` |
| `{uploadUrl}` | PUT | Raw file bytes | HTTP 200 |
| `/transfer/{id}/done` | POST | None | `{ok, id}` |
| `/i/{id}` | GET | None | `{ok, url, c?}` |
| `/health` | GET | None | `{ok}` |

---

## 📝 Supported Image Types

`type` field mein yeh values accepted hain (kuch bhi jo `image/` se start ho):

| MIME Type | Extension |
|-----------|-----------|
| `image/jpeg` | `.jpg`, `.jpeg` |
| `image/png` | `.png` |
| `image/gif` | `.gif` |
| `image/webp` | `.webp` |
| `image/svg+xml` | `.svg` |
| `image/bmp` | `.bmp` |
| `image/tiff` | `.tiff` |
| `image/avif` | `.avif` |

> Server sirf check karta hai ki `type` field `image/` se start hota hai ya nahi. Actual file content validation nahi hoti.

---

## ⏱️ Important Timeouts

| What | Duration |
|------|----------|
| Presigned Upload URL validity | **5 minutes** |
| Pending transfer in Redis | **5 minutes** |
| Image URL cache in Redis | **24 hours** |
| CORS preflight cache | **1 hour** |
