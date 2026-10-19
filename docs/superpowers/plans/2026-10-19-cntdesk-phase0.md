# CNT Desk Fase 0 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a functional, secure remote desktop application with video + input control, targeting <50ms LAN latency, using Rust + Tauri with hybrid P2P/relay networking.

**Architecture:** Rust workspace with four crates (core protocol, client app, server relay/auth, platform capture). Tauri v2 wraps React/WebGL frontend for video rendering. QUIC transport via quinn, H.264 hardware encoding, Ed25519 auth with central server generating unique IDs. Hybrid P2P with VPS relay fallback.

**Tech Stack:** Rust, Tauri v2, React+TypeScript, quinn (QUIC), tokio, DXGI/PipeWire/ScreenCaptureKit, NVENC/AMF/VAAPI/VideoToolbox, SQLite+SQLCipher, Argon2id, gRPC (tonic)

**Spec:** `docs/superpowers/specs/2026-10-19-cntdesk-design.md`

## Global Constraints

- Rust edition 2021, minimum stable toolchain
- Tauri v2.x (not v1)
- QUIC via quinn crate, TLS 1.3 mandatory
- H.264 Baseline/Main profile only for phase 0
- SQLCipher AES-256-CBC for server DB encryption
- Argon2id with m=64MB, t=3, p=4 for password hashing
- Ed25519 for keypair and session token signing
- ID format: XXX-XXX-XXX (9 digits, dash-separated)
- Binary size target: <10MB per platform
- Phase 0 scope: video + input only, no audio/files/chat
- Priority order: Windows > Linux > Mac
- All tests must pass before commit; no `#[ignore]` without ticket reference

## Review Focus

- Frame capture produces black/corrupt frames when display is locked or sleeping → capture module must detect and signal "no frame available" without crashing
- Hardware encoder initialization fails silently on VMs without GPU → encoder factory must return explicit error, never fall through to undefined state
- QUIC connection hangs indefinitely when firewall blocks UDP → transport layer must timeout within 3s and initiate TCP fallback automatically
- Server accepts duplicate ID generation requests under race condition → ID generation must be atomic with DB transaction, reject concurrent attempts
- Input events arrive out of order due to network jitter → viewer must reorder by timestamp_us before applying, drop events older than 200ms

---

### Task 1: Workspace Scaffolding and Dependency Setup

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/cntdesk-core/Cargo.toml`
- Create: `crates/cntdesk-core/src/lib.rs`
- Create: `crates/cntdesk-client/Cargo.toml`
- Create: `crates/cntdesk-client/src/main.rs`
- Create: `crates/cntdesk-server/Cargo.toml`
- Create: `crates/cntdesk-server/src/main.rs`
- Create: `crates/cntdesk-capture/Cargo.toml`
- Create: `crates/cntdesk-capture/src/lib.rs`
- Create: `.gitignore`
- Create: `rust-toolchain.toml`

**Interfaces:**
- Produces: workspace compiles with `cargo check`, all four crates exist as empty shells

- [ ] **Step 1: Write workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/cntdesk-core",
    "crates/cntdesk-client",
    "crates/cntdesk-server",
    "crates/cntdesk-capture",
]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
quinn = "0.11"
serde = { version = "1", features = ["derive"] }
bincode = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
thiserror = "1"
anyhow = "1"
ed25519-dalek = { version = "2", features = ["rand_core"] }
argon2 = "0.5"
rusqlite = { version = "0.31", features = ["bundled"] }
sqlcipher = "0.6"
rand = "0.8"
```

- [ ] **Step 2: Create each crate Cargo.toml with workspace dependency references**

Each crate uses `[package]` with `edition = "2021"`, references workspace deps via `{ workspace = true }`. cntdesk-core depends on serde, bincode, thiserror, ed25519-dalek, rand. cntdesk-capture depends on cntdesk-core, tracing, thiserror. cntdesk-server depends on cntdesk-core, tokio, quinn, rusqlite, sqlcipher, argon2, tracing, tracing-subscriber, anyhow. cntdesk-client depends on cntdesk-core, cntdesk-capture, tokio, quinn, tracing, tracing-subscriber, anyhow.

- [ ] **Step 3: Create stub lib.rs/main.rs files**

Each file contains only `//! Crate documentation` comment and empty `fn main()` or no content for libs. Enough for `cargo check` to succeed.

- [ ] **Step 4: Create rust-toolchain.toml pinning stable channel**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 5: Create .gitignore**

Standard Rust gitignore: `/target`, `*.swp`, `.env`, `*.db`, `*.db-shm`, `*.db-wal`.

- [ ] **Step 6: Verify workspace compiles**

Run: `cargo check --workspace`
Expected: No errors, all four crates checked successfully.

- [ ] **Step 7: Commit**

```bash
git init
git add -A
git commit -m "chore: scaffold workspace with four crates"
```

---

### Task 2: Core Protocol Types and Serialization

**Files:**
- Create: `crates/cntdesk-core/src/protocol.rs`
- Create: `crates/cntdesk-core/src/crypto.rs`
- Create: `crates/cntdesk-core/src/error.rs`
- Modify: `crates/cntdesk-core/src/lib.rs`
- Test: `crates/cntdesk-core/tests/protocol_test.rs`
- Test: `crates/cntdesk-core/tests/crypto_test.rs`

**Interfaces:**
- Consumes: nothing (foundation crate)
- Produces: `protocol::Message`, `protocol::InputEvent`, `protocol::FrameMetadata`, `crypto::KeyPair`, `crypto::SessionToken`, `error::CoreError`

- [ ] **Step 1: Write failing test for Message serialization roundtrip**

```rust
#[test]
fn test_message_roundtrip() {
    let msg = Message::AuthRequest { id: "123-456-789".into(), password_hash: vec![0u8; 32] };
    let bytes = msg.serialize();
    let decoded = Message::deserialize(&bytes).unwrap();
    assert_eq!(msg, decoded);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-core --test protocol_test`
Expected: FAIL — `Message` not defined.

- [ ] **Step 3: Implement protocol types in `protocol.rs`**

Define enum `Message` with variants: `AuthRequest { id: String, password_hash: Vec<u8> }`, `AuthResponse { success: bool, session_token: Option<Vec<u8>> }`, `IceCandidate { candidate: String }`, `FrameMetadata { seq: u64, timestamp_us: u64, width: u16, height: u16, keyframe: bool }`, `InputEvent(InputEvent)`, `Disconnect { reason: String }`. Derive Serialize/Deserialize, PartialEq, Debug. Implement `serialize() -> Vec<u8>` using bincode, `deserialize(&[u8]) -> Result<Self>` with CoreError. Define struct `InputEvent { event_type: InputType, timestamp_us: u64, data: Vec<u8> }` and enum `InputType { MouseMove, MouseDown, MouseUp, KeyDown, KeyUp, Scroll }`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p cntdesk-core --test protocol_test`
Expected: PASS.

- [ ] **Step 5: Write failing test for Ed25519 keypair generation and signing**

```rust
#[test]
fn test_keypair_sign_verify() {
    let kp = KeyPair::generate();
    let msg = b"test message";
    let sig = kp.sign(msg);
    assert!(kp.verify(msg, &sig));
    assert!(!kp.verify(b"wrong", &sig));
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test -p cntdesk-core --test crypto_test`
Expected: FAIL — `KeyPair` not defined.

- [ ] **Step 7: Implement crypto module in `crypto.rs`**

Struct `KeyPair` wrapping ed25519_dalek SigningKey/VerifyingKey. Methods: `generate() -> Self` using OsRng, `sign(&[u8]) -> Vec<u8>`, `verify(&[u8], &[u8]) -> bool`, `public_key_bytes() -> Vec<u8>`. Struct `SessionToken { token: Vec<u8>, expires_at: u64 }` with `sign(keypair: &KeyPair) -> Vec<u8>` and `verify(signed: &[u8], public_key: &[u8]) -> Result<Self>`.

- [ ] **Step 8: Implement error types in `error.rs`**

Enum `CoreError` with variants: `Serialization(String)`, `Crypto(String)`, `Protocol(String)`. Implement `std::fmt::Display` and `std::error::Error`. Implement `From<bincode::Error>` and `From<ed25519_dalek::SignatureError>`.

- [ ] **Step 9: Export modules in lib.rs**

```rust
pub mod protocol;
pub mod crypto;
pub mod error;
```

- [ ] **Step 10: Run all core tests**

Run: `cargo test -p cntdesk-core`
Expected: All tests PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/cntdesk-core/
git commit -m "feat(core): add protocol types, crypto, and error handling"
```

---

### Task 3: Server Database Layer with SQLCipher

**Files:**
- Create: `crates/cntdesk-server/src/db.rs`
- Create: `crates/cntdesk-server/src/config.rs`
- Modify: `crates/cntdesk-server/src/main.rs`
- Test: `crates/cntdesk-server/tests/db_test.rs`

**Interfaces:**
- Consumes: `cntdesk_core::crypto::KeyPair` (for storing public keys)
- Produces: `db::Database`, `db::DeviceRecord`, `config::ServerConfig`

- [ ] **Step 1: Write failing test for device registration and lookup**

```rust
#[test]
fn test_register_and_lookup_device() {
    let db = Database::open_in_memory("test-key").unwrap();
    let record = DeviceRecord {
        id: "123-456-789".into(),
        public_key: vec![0u8; 32],
        password_hash: None,
        created_at: 1000,
    };
    db.register_device(&record).unwrap();
    let found = db.get_device("123-456-789").unwrap().unwrap();
    assert_eq!(found.id, record.id);
    assert_eq!(found.public_key, record.public_key);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-server --test db_test`
Expected: FAIL — `Database` not defined.

- [ ] **Step 3: Implement config module in `config.rs`**

Struct `ServerConfig { db_path: String, db_key: String, listen_addr: String, tls_cert_path: String, tls_key_path: String }`. Implement `from_env() -> Result<Self>` reading env vars `CNTDESK_DB_PATH`, `CNTDESK_DB_KEY`, `CNTDESK_LISTEN_ADDR`, `CNTDESK_TLS_CERT`, `CNTDESK_TLS_KEY` with sensible defaults for development.

- [ ] **Step 4: Implement database module in `db.rs`**

Struct `Database` wrapping rusqlite::Connection with SQLCipher. Methods: `open(path: &str, key: &str) -> Result<Self>`, `open_in_memory(key: &str) -> Result<Self>` (for tests), `register_device(record: &DeviceRecord) -> Result<()>`, `get_device(id: &str) -> Result<Option<DeviceRecord>>`, `update_password(id: &str, hash: &[u8]) -> Result<()>`, `set_session_token(id: &str, token: &[u8], expires_at: u64) -> Result<()>`. Struct `DeviceRecord { id: String, public_key: Vec<u8>, password_hash: Option<Vec<u8>>, created_at: u64 }`. Execute CREATE TABLE IF NOT EXISTS on open. Use parameterized queries exclusively. Wrap register_device in transaction for atomicity.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p cntdesk-server --test db_test`
Expected: PASS.

- [ ] **Step 6: Write failing test for duplicate ID rejection**

```rust
#[test]
fn test_duplicate_id_rejected() {
    let db = Database::open_in_memory("test-key").unwrap();
    let record = DeviceRecord { id: "123-456-789".into(), public_key: vec![0u8; 32], password_hash: None, created_at: 1000 };
    db.register_device(&record).unwrap();
    let result = db.register_device(&record);
    assert!(result.is_err());
}
```

- [ ] **Step 7: Run test to verify it fails or passes depending on UNIQUE constraint**

Run: `cargo test -p cntdesk-server --test db_test`
Expected: If FAIL, add UNIQUE constraint on id column in CREATE TABLE. Re-run until PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/cntdesk-server/src/db.rs crates/cntdesk-server/src/config.rs crates/cntdesk-server/tests/db_test.rs
git commit -m "feat(server): add encrypted database layer with device registration"
```

---

### Task 4: Server Authentication and ID Generation

**Files:**
- Create: `crates/cntdesk-server/src/auth.rs`
- Create: `crates/cntdesk-server/src/idgen.rs`
- Modify: `crates/cntdesk-server/src/main.rs`
- Test: `crates/cntdesk-server/tests/auth_test.rs`

**Interfaces:**
- Consumes: `db::Database`, `cntdesk_core::crypto::KeyPair`, `cntdesk_core::protocol::Message`
- Produces: `auth::AuthService`, `idgen::IdGenerator`

- [ ] **Step 1: Write failing test for ID generation format and uniqueness**

```rust
#[test]
fn test_id_format_and_uniqueness() {
    let gen = IdGenerator::new();
    let id = gen.generate();
    assert!(regex::Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap().is_match(&id));
    let id2 = gen.generate();
    assert_ne!(id, id2);
}
```

Add `regex = "1"` to cntdesk-server dev-dependencies.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-server --test auth_test`
Expected: FAIL — `IdGenerator` not defined.

- [ ] **Step 3: Implement ID generator in `idgen.rs`**

Struct `IdGenerator` with AtomicU64 counter + Mutex<HashMap> for collision detection. Method `generate() -> String`: generates random 9-digit ID in XXX-XXX-XXX format using CSPRNG (rand::rngs::OsRng), checks against known IDs in DB, retries on collision. For phase 0, simple random generation is sufficient; counter-based reserved for future scaling.

- [ ] **Step 4: Write failing test for auth flow**

```rust
#[test]
fn test_auth_service_register_and_verify() {
    let db = Database::open_in_memory("key").unwrap();
    let service = AuthService::new(db);
    let pubkey = vec![1u8; 32];
    let id = service.register_new_device(&pubkey).unwrap();
    assert!(service.verify_device_exists(&id));
}
```

- [ ] **Step 5: Implement auth service in `auth.rs`**

Struct `AuthService { db: Database, id_gen: IdGenerator }`. Methods: `new(db: Database) -> Self`, `register_new_device(public_key: &[u8]) -> Result<String>` (generates ID, stores record, returns ID), `verify_password(id: &str, password_hash: &[u8]) -> Result<bool>` (uses argon2 verify), `set_permanent_password(id: &str, password: &[u8]) -> Result<()>` (hashes with Argon2id m=64MB t=3 p=4, stores), `generate_one_time_password(id: &str) -> Result<String>` (6-digit numeric via CSPRNG, hashes and stores, valid 1h), `create_session_token(id: &str, keypair: &KeyPair) -> Result<Vec<u8>>` (signs token with server keypair, stores expiry 24h).

- [ ] **Step 6: Run all auth tests**

Run: `cargo test -p cntdesk-server --test auth_test`
Expected: All PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/cntdesk-server/src/auth.rs crates/cntdesk-server/src/idgen.rs crates/cntdesk-server/tests/auth_test.rs
git commit -m "feat(server): add authentication service and ID generation"
```

---

### Task 5: QUIC Transport Layer with TCP Fallback

**Files:**
- Create: `crates/cntdesk-core/src/transport.rs`
- Modify: `crates/cntdesk-core/src/lib.rs`
- Test: `crates/cntdesk-core/tests/transport_test.rs`

**Interfaces:**
- Consumes: `protocol::Message`, `error::CoreError`
- Produces: `transport::Transport`, `transport::Connection`

- [ ] **Step 1: Write failing test for QUIC send/receive message**

```rust
#[tokio::test]
async fn test_quic_send_receive() {
    let (server, client) = Transport::loopback_pair().await.unwrap();
    let msg = Message::Disconnect { reason: "test".into() };
    client.send(&msg).await.unwrap();
    let received = server.recv().await.unwrap();
    assert_eq!(msg, received);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-core --test transport_test`
Expected: FAIL — `Transport` not defined.

- [ ] **Step 3: Implement transport module in `transport.rs`**

Struct `Transport` wrapping quinn::Endpoint. Methods: `bind(addr: &str, tls_config: TlsConfig) -> Result<Self>`, `connect(addr: &str, tls_config: TlsConfig) -> Result<Connection>`, `accept() -> Result<Connection>`, `loopback_pair() -> Result<(Transport, Connection)>` (for testing, self-signed cert). Struct `Connection` wrapping quinn::Connection with bidirectional stream. Methods: `send(msg: &Message) -> Result<()>` (serialize + write length-prefixed), `recv() -> Result<Message>` (read length prefix + deserialize), `close()`. Timeout: 3s for connection establishment. On QUIC failure, log warning and attempt TCP fallback (phase 0: return error; full TCP fallback implemented in Task 8).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p cntdesk-core --test transport_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cntdesk-core/src/transport.rs crates/cntdesk-core/tests/transport_test.rs
git commit -m "feat(core): add QUIC transport layer with loopback testing"
```

---

### Task 6: Platform Screen Capture Abstraction

**Files:**
- Create: `crates/cntdesk-capture/src/capture.rs`
- Create: `crates/cntdesk-capture/src/platform/mod.rs`
- Create: `crates/cntdesk-capture/src/platform/windows.rs`
- Create: `crates/cntdesk-capture/src/platform/linux.rs`
- Create: `crates/cntdesk-capture/src/platform/macos.rs`
- Modify: `crates/cntdesk-capture/src/lib.rs`
- Test: `crates/cntdesk-capture/tests/capture_test.rs`

**Interfaces:**
- Consumes: `cntdesk_core::error::CoreError`
- Produces: `capture::ScreenCapture`, `capture::CapturedFrame`, `capture::CaptureConfig`

- [ ] **Step 1: Write failing test for capture trait and config**

```rust
#[test]
fn test_capture_config_defaults() {
    let cfg = CaptureConfig::default();
    assert_eq!(cfg.target_fps, 30);
    assert_eq!(cfg.max_width, 1920);
    assert_eq!(cfg.max_height, 1080);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-capture --test capture_test`
Expected: FAIL — `CaptureConfig` not defined.

- [ ] **Step 3: Implement capture abstraction in `capture.rs`**

Struct `CaptureConfig { target_fps: u32, max_width: u32, max_height: u32 }` with Default impl (30fps, 1920x1080). Struct `CapturedFrame { data: Vec<u8>, width: u32, height: u32, format: PixelFormat, timestamp_us: u64 }`. Enum `PixelFormat { Bgra, Rgba }`. Trait `ScreenCaptureBackend: Send + Sync` with methods: `start(config: &CaptureConfig) -> Result<()>`, `next_frame() -> Result<Option<CapturedFrame>>` (returns None when display locked/sleeping), `stop() -> Result<()>`. Factory function `create_backend() -> Result<Box<dyn ScreenCaptureBackend>>` dispatching by platform via cfg flags.

- [ ] **Step 4: Create platform module stubs**

`platform/mod.rs` re-exports platform-specific backend behind `#[cfg(target_os)]`. Each platform file (`windows.rs`, `linux.rs`, `macos.rs`) contains struct implementing `ScreenCaptureBackend` with `todo!()` bodies for now. Windows uses DXGI types, Linux uses PipeWire types, macOS uses ScreenCaptureKit types. Compilation must succeed on current platform.

- [ ] **Step 5: Export modules in lib.rs**

```rust
pub mod capture;
mod platform;
pub use capture::{ScreenCaptureBackend, CapturedFrame, CaptureConfig, PixelFormat};
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p cntdesk-capture --test capture_test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/cntdesk-capture/
git commit -m "feat(capture): add screen capture abstraction with platform stubs"
```

---

### Task 7: Hardware Video Encoder Abstraction

**Files:**
- Create: `crates/cntdesk-core/src/encoder.rs`
- Modify: `crates/cntdesk-core/src/lib.rs`
- Test: `crates/cntdesk-core/tests/encoder_test.rs`

**Interfaces:**
- Consumes: `cntdesk_capture::CapturedFrame`, `error::CoreError`
- Produces: `encoder::VideoEncoder`, `encoder::EncodedPacket`, `encoder::EncoderConfig`

- [ ] **Step 1: Write failing test for encoder config and packet structure**

```rust
#[test]
fn test_encoder_config_validation() {
    let cfg = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        bitrate_kbps: 4000,
        keyframe_interval_ms: 1000,
    };
    assert!(cfg.validate().is_ok());
    let bad = EncoderConfig { width: 0, ..cfg };
    assert!(bad.validate().is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-core --test encoder_test`
Expected: FAIL — `EncoderConfig` not defined.

- [ ] **Step 3: Implement encoder abstraction in `encoder.rs`**

Struct `EncoderConfig { width: u32, height: u32, fps: u32, bitrate_kbps: u32, keyframe_interval_ms: u32 }` with `validate() -> Result<()>`. Struct `EncodedPacket { data: Vec<u8>, is_keyframe: bool, timestamp_us: u64, seq: u64 }`. Trait `VideoEncoderBackend: Send` with methods: `new(config: &EncoderConfig) -> Result<Self>`, `encode(frame: &CapturedFrame) -> Result<EncodedPacket>`, `request_keyframe()`, `update_bitrate(kbps: u32)`. Factory function `create_encoder(config: &EncoderConfig) -> Result<Box<dyn VideoEncoderBackend>>`: tries hardware encoders in order (NVENC → AMF → VAAPI → VideoToolbox), falls back to x264 software if all fail. Log which encoder selected at INFO level.

- [ ] **Step 4: Export in lib.rs**

Add `pub mod encoder;` to existing exports.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p cntdesk-core --test encoder_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/cntdesk-core/src/encoder.rs crates/cntdesk-core/tests/encoder_test.rs
git commit -m "feat(core): add video encoder abstraction with HW/SW fallback chain"
```

---

### Task 8: Server Relay and Signaling Service

**Files:**
- Create: `crates/cntdesk-server/src/relay.rs`
- Create: `crates/cntdesk-server/src/signaling.rs`
- Modify: `crates/cntdesk-server/src/main.rs`
- Test: `crates/cntdesk-server/tests/relay_test.rs`

**Interfaces:**
- Consumes: `db::Database`, `auth::AuthService`, `cntdesk_core::transport::Transport`, `cntdesk_core::protocol::Message`
- Produces: `relay::RelayService`, `signaling::SignalingService`

- [ ] **Step 1: Write failing test for signaling message routing**

```rust
#[tokio::test]
async fn test_signaling_routes_ice_candidates() {
    let (svc, _transport) = SignalingService::new_test().await;
    // Verify ICE candidate from peer A reaches peer B session
    svc.route_message("session-1", Message::IceCandidate { candidate: "test".into() }).await.unwrap();
    let routed = svc.get_pending_messages("session-1").await;
    assert_eq!(routed.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-server --test relay_test`
Expected: FAIL — `SignalingService` not defined.

- [ ] **Step 3: Implement signaling service in `signaling.rs`**

Struct `SignalingService` managing active sessions as `HashMap<String, SessionState>` behind RwLock. SessionState holds peer IDs, pending messages queue, creation timestamp. Methods: `new(db: Database) -> Self`, `create_session(peer_a: &str, peer_b: &str) -> Result<String>` (returns session ID), `route_message(session_id: &str, msg: Message) -> Result<()>`, `get_pending_messages(session_id: &str) -> Vec<Message>`, `close_session(session_id: &str)`. Sessions expire after 24h via background cleanup task.

- [ ] **Step 4: Implement relay service in `relay.rs`**

Struct `RelayService` wrapping Transport endpoint. Methods: `new(transport: Transport, signaling: SignalingService) -> Self`, `run() -> Result<()>` (accept loop: accept connection → authenticate via AuthService → register in signaling → spawn bidirectional forward task). Forward task reads Messages from one peer, routes through signaling or directly forwards to other peer if P2P failed. Bandwidth tracking per session for future adaptive bitrate. Graceful shutdown on SIGTERM/SIGINT.

- [ ] **Step 5: Wire server main.rs**

Load ServerConfig from env, open Database, create AuthService, bind Transport, create SignalingService + RelayService, call `relay.run().await`. Setup tracing-subscriber with JSON format.

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p cntdesk-server --test relay_test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/cntdesk-server/src/relay.rs crates/cntdesk-server/src/signaling.rs crates/cntdesk-server/src/main.rs crates/cntdesk-server/tests/relay_test.rs
git commit -m "feat(server): add relay and signaling service with session management"
```

---

### Task 9: Client Connection Manager and Auth Flow

**Files:**
- Create: `crates/cntdesk-client/src/connection.rs`
- Create: `crates/cntdesk-client/src/auth.rs`
- Modify: `crates/cntdesk-client/src/main.rs`
- Test: `crates/cntdesk-client/tests/connection_test.rs`

**Interfaces:**
- Consumes: `cntdesk_core::transport::{Transport, Connection}`, `cntdesk_core::protocol::Message`, `cntdesk_core::crypto::KeyPair`
- Produces: `connection::ConnectionManager`, `auth::ClientAuth`

- [ ] **Step 1: Write failing test for client auth registration flow**

```rust
#[tokio::test]
async fn test_client_first_time_registration() {
    let auth = ClientAuth::new_temp_dir().unwrap();
    assert!(!auth.has_identity());
    let kp = auth.ensure_identity().unwrap();
    assert!(auth.has_identity());
    assert_eq!(kp.public_key_bytes().len(), 32);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cntdesk-client --test connection_test`
Expected: FAIL — `ClientAuth` not defined.

- [ ] **Step 3: Implement client auth in `auth.rs`**

Struct `ClientAuth` managing local identity persistence. Methods: `new(config_dir: PathBuf) -> Result<Self>`, `new_temp_dir() -> Result<Self>` (for tests), `has_identity() -> bool`, `ensure_identity() -> Result<KeyPair>` (generates Ed25519 keypair on first run, persists private key encrypted with OS keyring or file-level encryption, returns KeyPair), `load_identity() -> Result<KeyPair>`. Private key stored at `{config_dir}/identity.key` with 0600 permissions.

- [ ] **Step 4: Implement connection manager in `connection.rs`**

Struct `ConnectionManager { transport: Transport, auth: ClientAuth, server_addr: String }`. Methods: `new(server_addr: &str, auth: ClientAuth) -> Result<Self>`, `register_with_server() -> Result<String>` (sends public key, receives ID, persists locally), `connect_to_peer(id: &str, password: &str) -> Result<PeerSession>`, `host_session() -> Result<HostSession>`. PeerSession/HostSession are typed wrappers around Connection with role-specific message handling. Retry with exponential backoff (max 30s) on connection failure. TCP fallback triggered after QUIC timeout.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p cntdesk-client --test connection_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/cntdesk-client/src/connection.rs crates/cntdesk-client/src/auth.rs crates/cntdesk-client/tests/connection_test.rs
git commit -m "feat(client): add connection manager and local identity management"
```

---

### Task 10: Tauri App Shell and IPC Bridge

**Files:**
- Create: `crates/cntdesk-client/src/ipc.rs`
- Create: `crates/cntdesk-client/src/commands.rs`
- Create: `crates/cntdesk-client/tauri.conf.json`
- Create: `crates/cntdesk-client/build.rs`
- Modify: `crates/cntdesk-client/src/main.rs`
- Modify: `crates/cntdesk-client/Cargo.toml`

**Interfaces:**
- Consumes: `connection::ConnectionManager`, `auth::ClientAuth`, `cntdesk_core::protocol::InputEvent`
- Produces: Tauri commands callable from frontend JS

- [ ] **Step 1: Add Tauri dependencies to Cargo.toml**

Add to cntdesk-client Cargo.toml: `tauri = { version = "2", features = [] }`, `tauri-build = { version = "2", features = [] }` as build-dependency.

- [ ] **Step 2: Create tauri.conf.json**

Minimal config: app name "CNT Desk", identifier "com.cntdesk.app", window title "CNT Desk", width 1280, height 720, resizable true. Security: CSP allowing only local resources. Build: devPath "../ui/dist", distDir "../ui/dist".

- [ ] **Step 3: Create build.rs**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 4: Implement IPC commands in `commands.rs`**

Tauri commands (all async, return Result<T, String>): `get_device_id() -> Option<String>`, `set_password(password: String) -> Result<()>`, `generate_one_time_password() -> Result<String>`, `connect(remote_id: String, password: String) -> Result<()>`, `disconnect() -> Result<()>`, `send_input(event: InputEvent) -> Result<()>`, `get_connection_state() -> ConnectionState`. Each command accesses ConnectionManager via Tauri State.

- [ ] **Step 5: Implement IPC bridge in `ipc.rs`**

Struct `AppIpc` wrapping ConnectionManager + ClientAuth in Arc<Mutex>. Implements Tauri plugin or state setup. Registers all commands with Tauri builder. Handles graceful shutdown on window close.

- [ ] **Step 6: Update main.rs for Tauri entry point**

Replace tokio::main with tauri::Builder pattern. Register AppIpc state, register commands, setup tracing. Keep existing logic accessible via IPC.

- [ ] **Step 7: Verify Tauri compiles**

Run: `cargo check -p cntdesk-client`
Expected: No errors.

- [ ] **Step 8: Commit**

```bash
git add crates/cntdesk-client/
git commit -m "feat(client): add Tauri app shell with IPC bridge"
```

---

### Task 11: React Frontend with WebGL Video Renderer

**Files:**
- Create: `ui/package.json`
- Create: `ui/tsconfig.json`
- Create: `ui/vite.config.ts`
- Create: `ui/src/main.tsx`
- Create: `ui/src/App.tsx`
- Create: `ui/src/components/VideoCanvas.tsx`
- Create: `ui/src/components/ConnectionPanel.tsx`
- Create: `ui/src/hooks/useTauriIpc.ts`
- Create: `ui/src/types/ipc.ts`
- Create: `ui/index.html`

**Interfaces:**
- Consumes: Tauri IPC commands from Task 10
- Produces: rendered video frames on canvas, input events sent to Rust backend

- [ ] **Step 1: Initialize React+TypeScript project**

Create package.json with dependencies: react, react-dom, @tauri-apps/api, typescript, vite, @vitejs/plugin-react. Scripts: dev, build, preview. tsconfig with strict mode, jsx react-jsx.

- [ ] **Step 2: Create vite.config.ts**

Configure @vitejs/plugin-react, set clearScreen false, define TAURI_PLATFORM/TARGET env vars.

- [ ] **Step 3: Create IPC types and hook**

`types/ipc.ts`: TypeScript interfaces matching Rust command signatures. `hooks/useTauriIpc.ts`: wrapper around @tauri-apps/api invoke with error handling and loading states.

- [ ] **Step 4: Implement VideoCanvas component**

WebGL-based renderer: creates WebGL context, uploads YUV/NV12 frame data as textures, applies color conversion shader, renders to fullscreen quad. Handles resize, maintains aspect ratio. Accepts frame buffer from Rust via IPC event listener. Target: <2ms render time per frame.

- [ ] **Step 5: Implement ConnectionPanel component**

UI for: device ID display, password input fields (permanent + one-time), connect button with remote ID + password inputs, connection status indicator, disconnect button. Uses useTauriIpc hook for all interactions.

- [ ] **Step 6: Implement App.tsx and main.tsx**

App layout: ConnectionPanel sidebar (collapsible), VideoCanvas main area. main.tsx mounts React root. Input event capture: mousemove/mousedown/mouseup/keydown/keyup/wheel listeners on VideoCanvas, translated to InputEvent format, sent via send_input IPC.

- [ ] **Step 7: Create index.html**

Standard HTML5 shell with div#root, script src="/src/main.tsx".

- [ ] **Step 8: Verify frontend builds**

Run: `cd ui && npm install && npm run build`
Expected: dist/ created without errors.

- [ ] **Step 9: Commit**

```bash
git add ui/
git commit -m "feat(ui): add React frontend with WebGL video renderer"
```

---

### Task 12: Integration Testing and End-to-End Verification

**Files:**
- Create: `tests/integration/e2e_test.rs`
- Create: `tests/integration/fixtures.rs`
- Create: `scripts/run-integration.sh`

**Interfaces:**
- Consumes: all crates, server binary, client binary
- Produces: passing integration test suite verifying full connection flow

- [ ] **Step 1: Create integration test fixtures**

`fixtures.rs`: helper functions to start test server (in-memory DB, self-signed TLS cert, ephemeral port), create test client instances, generate test keypairs, wait for server ready.

- [ ] **Step 2: Write end-to-end connection test**

```rust
#[tokio::test]
async fn test_full_connection_flow() {
    let server = TestServer::start().await;
    let host = TestClient::new(&server.addr()).await;
    let viewer = TestClient::new(&server.addr()).await;
    
    // Host registers and gets ID
    let host_id = host.register().await.unwrap();
    assert!(regex::Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap().is_match(&host_id));
    
    // Host sets password
    host.set_permanent_password("testpass123").await.unwrap();
    
    // Viewer connects to host
    viewer.connect(&host_id, "testpass123").await.unwrap();
    
    // Verify connection established
    assert!(viewer.is_connected().await);
    assert!(host.has_active_viewer().await);
    
    // Disconnect
    viewer.disconnect().await.unwrap();
    assert!(!viewer.is_connected().await);
}
```

- [ ] **Step 3: Write wrong-password rejection test**

Test that connecting with incorrect password returns auth error and does not establish session.

- [ ] **Step 4: Write duplicate-ID atomicity test**

Spawn 10 concurrent registration requests, verify all get unique IDs and no DB corruption.

- [ ] **Step 5: Create integration test runner script**

`scripts/run-integration.sh`: starts server, runs `cargo test --test e2e_test`, reports results, cleans up.

- [ ] **Step 6: Run full integration suite**

Run: `bash scripts/run-integration.sh`
Expected: All tests PASS.

- [ ] **Step 7: Final commit**

```bash
git add tests/ scripts/
git commit -m "test: add end-to-end integration tests for full connection flow"
```

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-10-19-cntdesk-phase0.md`. User instructed to proceed without stopping for review. Executing via superpowers:subagent-driven-development (recommended approach: 12 tasks with sequential dependencies on core protocol types, fresh reviewer per task prevents cascading interface mismatches across crates).