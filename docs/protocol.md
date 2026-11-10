# Zima Connect Wire Protocol Specification

Version: 1.0  
Encoding: MessagePack (binary payloads) & JSON (WebSocket control stream)  
Encryption: X25519 + AES-256-GCM + Ed25519 (End-to-End)

---

## 1. Overview

The Zima Connect protocol operates over a persistent WebSocket connection between client devices and the self-hosted ZimaOS server.

- **Clients** initiate connections to the server via TLS (WSS / HTTPS).
- **Server** acts as a semi-trusted relay and store-and-forward queue.
- **Payloads** between peers are encrypted end-to-end; the server cannot decrypt the content of envelopes.

---

## 2. Envelope Specification

Every packet passed through the network follows the MessagePack/JSON envelope schema:

```json
{
  "version": 1,
  "id": "018f4a38-7299-7811-9a4f-56191bca7201",
  "from": "a8f3b2019c4d5e6f...",
  "to": "b90123ef456a7b8c...",
  "type": "clipboard.sync",
  "timestamp": 1731245900123,
  "ttl": 172800,
  "encrypted": true,
  "payload": "<bytes>",
  "signature": "<64-byte Ed25519 signature>"
}
```

### Fields:
- `version` (`u8`): Protocol version (currently `1`).
- `id` (`UUIDv7`): Timestamp-sortable UUID v7.
- `from` (`DeviceID`): Hex-encoded SHA-256 hash of sender's X25519 public key.
- `to` (`DeviceID`): Hex-encoded SHA-256 hash of recipient's X25519 public key.
- `type` (`String`): Plugin message type identifier (e.g., `clipboard.sync`, `file.offer`, `notification.post`).
- `timestamp` (`i64`): Unix millisecond timestamp.
- `ttl` (`u32`): Time-to-live in seconds before server discards staged envelope. `0` = default (48h).
- `encrypted` (`bool`): Whether `payload` is encrypted with the recipient's session key.
- `payload` (`bytes`): Serialized plugin data or ciphertext.
- `signature` (`bytes`): Ed25519 signature of the envelope header and ciphertext.

---

## 3. Pairing Flow & Key Exchange

1. **Initiation**: Device A requests pairing with Device B via the server or by scanning a QR code.
2. **ECDH**: Both devices generate ephemeral X25519 keypairs.
3. **SAS Verification**:
   - Both devices derive a Shared Secret: `shared_secret = X25519(sk_A, pk_B)`
   - HKDF-SHA256 expands the shared secret into:
     - `session_key` (32 bytes AES-256-GCM)
     - `sas_code` (6-digit decimal PIN displayed to the user)
4. **Approval**: User verifies that both devices display identical 6-digit PINs and approves.
5. **Storage**: Both devices save each other's long-term identity public key in OS Keyring / Android Keystore.

---

## 4. File Transfer Protocol

1. **Offer**: Sender calculates file BLAKE3 checksum, encrypts file in 4MB chunks using recipient's session key.
2. **Staging Upload**:
   - `POST /api/v1/files/upload`
   - Server stores ciphertext in `/DATA/AppData/zima-connect/data/staging/<ref_id>`.
3. **Notification**: Sender emits `file.offer` envelope over WebSocket with `ref_id`, `filename`, `size`, `checksum`.
4. **Download**:
   - If recipient is online, it downloads immediately via `GET /api/v1/files/<ref_id>/download`.
   - If recipient is offline, the server holds the file until reconnect (up to 48 hours).
   - Once recipient acknowledges successful receipt, the server deletes the file from staging.
