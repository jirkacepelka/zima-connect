# 🔗 Zima Connect

**Modern device connectivity platform** — a self-hosted alternative to KDE Connect with Peer-to-Server-to-Peer architecture.

> Your devices. Your server. Your data.

## ✨ Key Features

- **Works across networks** — no need for devices to be on the same WiFi
- **Offline delivery** — server holds files and messages until the target device comes online
- **End-to-end encrypted** — the server relays but cannot read your data
- **Self-hosted** — runs on your ZimaOS device, no cloud dependency
- **Plugin architecture** — clipboard sync, file transfer, notifications, media control, and more

## 🏗️ Architecture

```
┌─────────────┐     ┌──────────────────┐     ┌─────────────┐
│  📱 Android  │◄───►│  🖥️ ZimaOS Server │◄───►│  🖥️ Desktop  │
│  (Kotlin)   │     │  (Go + SQLite)   │     │  (Tauri 2)  │
└─────────────┘     └──────────────────┘     └─────────────┘
        │                    │                       │
        └────────────────────┼───────────────────────┘
                     Rust Core Library
                  (Crypto + Protocol)
```

## 📦 Components

| Component | Technology | Directory |
|-----------|-----------|-----------|
| **Core Library** | Rust (crypto, protocol, serialization) | `core/` |
| **Server** | Go (Fiber, WebSocket, SQLite) | `server/` |
| **Desktop App** | Tauri 2.0 + SvelteKit | `desktop/` |
| **Android App** | Kotlin + Jetpack Compose | `android/` |
| **Web Dashboard** | SvelteKit | `dashboard/` |

## 🔌 Plugins

| Plugin | Description | Status |
|--------|-------------|--------|
| Clipboard Sync | Bidirectional clipboard sharing | 🚧 |
| File Transfer | Send files with offline queuing | 🚧 |
| Notification Mirror | Mirror phone notifications to desktop | 📋 |
| Media Control | Control media players remotely | 📋 |
| Battery Status | Monitor remote device battery | 📋 |
| Find Device | Ring a device to find it | 📋 |
| SMS Sync | Read and send SMS from desktop | 📋 |
| Remote Input | Mouse/keyboard from phone | 📋 |

## 🚀 Quick Start

### Server (ZimaOS / Docker)

```bash
docker compose up -d
```

The server runs on port `9120` (API) and `9121` (Dashboard).

### Desktop (Windows / Linux)

Download from [Releases](https://github.com/zima-connect/desktop/releases) or build from source:

```bash
cd desktop
npm install
npm run tauri build
```

### Android

Download from [Google Play](https://play.google.com/store/apps/details?id=dev.zimaconnect) or build from source.

## 🔐 Security

- **Transport**: TLS 1.3 (HTTPS + WSS)
- **End-to-End**: X25519 key exchange + AES-256-GCM encryption
- **Authentication**: JWT + device public keys
- **Pairing**: Short Authentication String (6-digit PIN / QR code)
- **Key Storage**: OS Keychain / Android Keystore / Linux Kernel Keyring

## 📄 License

GPL-3.0 — see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

Inspired by [KDE Connect](https://kdeconnect.kde.org/) — thank you for pioneering device connectivity on Linux.
