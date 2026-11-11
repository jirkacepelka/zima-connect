//! The protobuf messages Quick Share speaks (Nearby Connections, UKEY2, SecureMessage and the
//! sharing layer on top). Written by hand with prost so building needs no `protoc`.
//! Field numbers follow the public definitions the Nearby project and its ports use.
#![allow(clippy::enum_variant_names)]

use prost::{Enumeration, Message};

// ───────────────────────── Nearby Connections ("offline" frames) ─────────────────────────

#[derive(Clone, PartialEq, Message)]
pub struct OfflineFrame {
    #[prost(int32, optional, tag = "1")]
    pub version: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub v1: Option<V1Frame>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum FrameType {
    Unknown = 0,
    ConnectionRequest = 1,
    ConnectionResponse = 2,
    PayloadTransfer = 3,
    BandwidthUpgradeNegotiation = 4,
    KeepAlive = 5,
    Disconnection = 6,
    PairedKeyEncryption = 7,
}

#[derive(Clone, PartialEq, Message)]
pub struct V1Frame {
    #[prost(enumeration = "FrameType", optional, tag = "1")]
    pub r#type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub connection_request: Option<ConnectionRequestFrame>,
    #[prost(message, optional, tag = "3")]
    pub connection_response: Option<ConnectionResponseFrame>,
    #[prost(message, optional, tag = "4")]
    pub payload_transfer: Option<PayloadTransferFrame>,
    #[prost(message, optional, tag = "6")]
    pub keep_alive: Option<KeepAliveFrame>,
    #[prost(message, optional, tag = "7")]
    pub disconnection: Option<DisconnectionFrame>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ConnectionRequestFrame {
    #[prost(string, optional, tag = "1")]
    pub endpoint_id: Option<String>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub endpoint_name: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub handshake_data: Option<Vec<u8>>,
    #[prost(int32, optional, tag = "4")]
    pub nonce: Option<i32>,
    /// 5 = WIFI_LAN
    #[prost(int32, repeated, packed = "false", tag = "5")]
    pub mediums: Vec<i32>,
    #[prost(bytes = "vec", optional, tag = "6")]
    pub endpoint_info: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum ResponseStatus {
    Unknown = 0,
    Accept = 1,
    Reject = 2,
}

#[derive(Clone, PartialEq, Message)]
pub struct ConnectionResponseFrame {
    #[prost(enumeration = "ResponseStatus", optional, tag = "3")]
    pub response: Option<i32>,
    #[prost(message, optional, tag = "4")]
    pub os_info: Option<OsInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct OsInfo {
    /// 3 = Windows, 100 = Linux, 4 = Apple, 1 = Android
    #[prost(int32, optional, tag = "1")]
    pub r#type: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum PacketType {
    Unknown = 0,
    Data = 1,
    Control = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum PayloadType {
    Unknown = 0,
    Bytes = 1,
    File = 2,
    Stream = 3,
}

#[derive(Clone, PartialEq, Message)]
pub struct PayloadHeader {
    #[prost(int64, optional, tag = "1")]
    pub id: Option<i64>,
    #[prost(enumeration = "PayloadType", optional, tag = "2")]
    pub r#type: Option<i32>,
    #[prost(int64, optional, tag = "3")]
    pub total_size: Option<i64>,
    #[prost(bool, optional, tag = "4")]
    pub is_sensitive: Option<bool>,
    #[prost(string, optional, tag = "5")]
    pub file_name: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub parent_folder: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PayloadChunk {
    /// 1 = last chunk
    #[prost(int32, optional, tag = "1")]
    pub flags: Option<i32>,
    #[prost(int64, optional, tag = "2")]
    pub offset: Option<i64>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ControlMessage {
    #[prost(int32, optional, tag = "1")]
    pub event: Option<i32>,
    #[prost(int64, optional, tag = "2")]
    pub offset: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PayloadTransferFrame {
    #[prost(enumeration = "PacketType", optional, tag = "1")]
    pub packet_type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub payload_header: Option<PayloadHeader>,
    #[prost(message, optional, tag = "3")]
    pub payload_chunk: Option<PayloadChunk>,
    #[prost(message, optional, tag = "4")]
    pub control_message: Option<ControlMessage>,
}

#[derive(Clone, PartialEq, Message)]
pub struct KeepAliveFrame {
    #[prost(bool, optional, tag = "1")]
    pub ack: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct DisconnectionFrame {
    #[prost(bool, optional, tag = "1")]
    pub request_safe_to_disconnect: Option<bool>,
    #[prost(bool, optional, tag = "2")]
    pub ack_safe_to_disconnect: Option<bool>,
}

// ───────────────────────── UKEY2 handshake ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum Ukey2Type {
    UnknownDoNotUse = 0,
    Alert = 1,
    ClientInit = 2,
    ServerInit = 3,
    ClientFinish = 4,
}

#[derive(Clone, PartialEq, Message)]
pub struct Ukey2Message {
    #[prost(enumeration = "Ukey2Type", optional, tag = "1")]
    pub message_type: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub message_data: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Ukey2Alert {
    #[prost(int32, optional, tag = "1")]
    pub r#type: Option<i32>,
    #[prost(string, optional, tag = "2")]
    pub error_message: Option<String>,
}

/// UKEY2 alert codes we send.
pub mod alert {
    pub const BAD_MESSAGE_TYPE: i32 = 2;
    pub const BAD_VERSION: i32 = 100;
    pub const BAD_RANDOM: i32 = 101;
    pub const BAD_HANDSHAKE_CIPHER: i32 = 102;
    pub const BAD_NEXT_PROTOCOL: i32 = 103;
}

/// P256_SHA512
pub const CIPHER_P256_SHA512: i32 = 100;

#[derive(Clone, PartialEq, Message)]
pub struct CipherCommitment {
    #[prost(int32, optional, tag = "1")]
    pub handshake_cipher: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub commitment: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Ukey2ClientInit {
    #[prost(int32, optional, tag = "1")]
    pub version: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub random: Option<Vec<u8>>,
    #[prost(message, repeated, tag = "3")]
    pub cipher_commitments: Vec<CipherCommitment>,
    #[prost(string, optional, tag = "4")]
    pub next_protocol: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Ukey2ServerInit {
    #[prost(int32, optional, tag = "1")]
    pub version: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub random: Option<Vec<u8>>,
    #[prost(int32, optional, tag = "3")]
    pub handshake_cipher: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "4")]
    pub public_key: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Ukey2ClientFinished {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub public_key: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GenericPublicKey {
    /// 1 = EC_P256
    #[prost(int32, optional, tag = "1")]
    pub r#type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub ec_p256_public_key: Option<EcP256PublicKey>,
}

#[derive(Clone, PartialEq, Message)]
pub struct EcP256PublicKey {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub x: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub y: Option<Vec<u8>>,
}

// ───────────────────────── SecureMessage / device-to-device ─────────────────────────

#[derive(Clone, PartialEq, Message)]
pub struct SecureMessage {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub header_and_body: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub signature: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Header {
    /// 1 = HMAC_SHA256
    #[prost(int32, optional, tag = "1")]
    pub signature_scheme: Option<i32>,
    /// 2 = AES_256_CBC
    #[prost(int32, optional, tag = "2")]
    pub encryption_scheme: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "5")]
    pub iv: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "6")]
    pub public_metadata: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct HeaderAndBody {
    #[prost(message, optional, tag = "1")]
    pub header: Option<Header>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GcmMetadata {
    /// 13 = DEVICE_TO_DEVICE_MESSAGE
    #[prost(int32, optional, tag = "1")]
    pub r#type: Option<i32>,
    #[prost(int32, optional, tag = "2")]
    pub version: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct DeviceToDeviceMessage {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub message: Option<Vec<u8>>,
    #[prost(int32, optional, tag = "2")]
    pub sequence_number: Option<i32>,
}

// ───────────────────────── Sharing layer (what is offered, accepted) ─────────────────────────

#[derive(Clone, PartialEq, Message)]
pub struct Frame {
    #[prost(int32, optional, tag = "1")]
    pub version: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub v1: Option<SharingV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum SharingType {
    Unknown = 0,
    Introduction = 1,
    Response = 2,
    PairedKeyEncryption = 3,
    PairedKeyResult = 4,
    CertificateInfo = 5,
    Cancel = 6,
}

#[derive(Clone, PartialEq, Message)]
pub struct SharingV1 {
    #[prost(enumeration = "SharingType", optional, tag = "1")]
    pub r#type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub introduction: Option<IntroductionFrame>,
    #[prost(message, optional, tag = "3")]
    pub connection_response: Option<SharingResponse>,
    #[prost(message, optional, tag = "4")]
    pub paired_key_encryption: Option<PairedKeyEncryptionFrame>,
    #[prost(message, optional, tag = "5")]
    pub paired_key_result: Option<PairedKeyResultFrame>,
}

#[derive(Clone, PartialEq, Message)]
pub struct IntroductionFrame {
    #[prost(message, repeated, tag = "1")]
    pub file_metadata: Vec<FileMetadata>,
    #[prost(message, repeated, tag = "2")]
    pub text_metadata: Vec<TextMetadata>,
    #[prost(string, optional, tag = "3")]
    pub required_package: Option<String>,
    #[prost(message, repeated, tag = "4")]
    pub wifi_credentials_metadata: Vec<WifiCredentialsMetadata>,
}

/// 0 unknown, 1 image, 2 video, 3 app, 4 audio
#[derive(Clone, PartialEq, Message)]
pub struct FileMetadata {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(int32, optional, tag = "2")]
    pub r#type: Option<i32>,
    #[prost(int64, optional, tag = "3")]
    pub payload_id: Option<i64>,
    #[prost(int64, optional, tag = "4")]
    pub size: Option<i64>,
    #[prost(string, optional, tag = "5")]
    pub mime_type: Option<String>,
    #[prost(int64, optional, tag = "6")]
    pub id: Option<i64>,
}

/// 0 unknown, 1 text, 2 url, 3 address, 4 phone number
#[derive(Clone, PartialEq, Message)]
pub struct TextMetadata {
    #[prost(string, optional, tag = "2")]
    pub text_title: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub r#type: Option<i32>,
    #[prost(int64, optional, tag = "4")]
    pub payload_id: Option<i64>,
    #[prost(int64, optional, tag = "5")]
    pub size: Option<i64>,
    #[prost(int64, optional, tag = "6")]
    pub id: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct WifiCredentialsMetadata {
    #[prost(string, optional, tag = "2")]
    pub ssid: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub security_type: Option<i32>,
    #[prost(int64, optional, tag = "4")]
    pub payload_id: Option<i64>,
    #[prost(int64, optional, tag = "5")]
    pub id: Option<i64>,
}

/// 1 accept, 2 reject, 3 not enough space, 4 unsupported attachment, 5 timed out
#[derive(Clone, PartialEq, Message)]
pub struct SharingResponse {
    #[prost(int32, optional, tag = "1")]
    pub status: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PairedKeyEncryptionFrame {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub signed_data: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    pub secret_id_hash: Option<Vec<u8>>,
}

/// 1 success, 2 fail, 3 unable
#[derive(Clone, PartialEq, Message)]
pub struct PairedKeyResultFrame {
    #[prost(int32, optional, tag = "1")]
    pub status: Option<i32>,
}
