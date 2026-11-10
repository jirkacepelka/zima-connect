# Zima Connect Server REST & WebSocket API Reference

Base URL: `http://<zimaos-host>:9120/api/v1`  
WebSocket Endpoint: `ws://<zimaos-host>:9120/api/v1/ws?token=<jwt>`  
Dashboard URL: `http://<zimaos-host>:9121`

---

## Authentication

### Register Device
`POST /auth/register`

Register a new device to the server.

**Request:**
```json
{
  "name": "Pixel 8 Pro",
  "platform": "android",
  "public_key": "9f8a7c6b5d4e..."
}
```

**Response (201 Created):**
```json
{
  "device_id": "018f4a38-7299-7811-9a4f-56191bca7201",
  "token": "eyJhbGciOi...",
  "server_name": "ZimaOS Hub"
}
```

---

### Initiate Pairing
`POST /auth/pair`

Start a pairing request to generate a 6-digit confirmation PIN.

---

### Confirm Pairing
`POST /auth/pair/confirm`

```json
{
  "device_id": "018f4a38...",
  "target_device_id": "018f4a39...",
  "pin": "847291"
}
```

---

## Devices

### List Paired Devices
`GET /devices`  
*Requires Authorization: Bearer <token>*

Returns list of all paired devices with live presence flags.

### Device Status
`GET /devices/:id/status`

---

## Files (Staging & Offline Storage)

### Upload File
`POST /files/upload`  
*Multipart form-data:*
- `to_device`: Target Device UUID
- `file`: Encrypted binary stream

**Response (201 Created):**
```json
{
  "ref_id": "f-9012-ab78",
  "filename": "archive.tar.gz",
  "size": 104857600,
  "checksum": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "expires_at": "2026-11-12T14:30:00Z"
}
```

### Download Staged File
`GET /files/:ref/download`

---

## Offline Message Queue

### Pending Messages
`GET /messages/pending`

### Send Message (HTTP Fallback)
`POST /messages/send`

### Acknowledge Message
`POST /messages/:id/ack`

---

## WebSocket Protocol

### Connection
Connect to `GET /api/v1/ws?token=<jwt>`

### Outgoing Messages (Client → Server)
```json
{
  "action": "send",
  "envelope": {
    "to": "dev-123",
    "type": "clipboard.sync",
    "payload": "..."
  }
}
```

```json
{
  "action": "ack",
  "message_id": "018f4a38-..."
}
```

```json
{
  "action": "ping"
}
```

### Incoming Events (Server → Client)
- `message`: Incoming envelope
- `delivered`: Sender receipt confirmation
- `queued`: Notifies sender that target is offline and message is held on server
- `device_online`: Target device came online
- `device_offline`: Target device disconnected
- `pong`: Keepalive response
