# CNT Desk — Design Document (Fase 0)

**Data:** 2026-10-19
**Status:** Draft para aprovação
**Autor:** Claude Opus 5.5 + Robson

---

## 1. Intent e Why

CNT Desk é aplicativo de acesso remoto com latência mínima, segurança forte e peso leve. Nasce da dor: soluções gratuitas (AnyDesk, TeamViewer) são limitadas; soluções pagas são caras; alternativas open-source (RustDesk) não atendem requisitos de produto comercial próprio. Objetivo fase 0: conexão única funcional e segura entre dois computadores, prioridade Windows > Linux > Mac. Futuro: licenciamento empresas, múltiplas conexões, áudio, categorias/tags, instaladores customizados.

## 2. Goals, Non-Goals, Anti-Goals

### Goals (Fase 0)
- Conexão remota funcional vídeo + input (mouse/teclado)
- Latência alvo < 50ms em LAN, < 150ms em WAN com relay
- Segurança: auth via servidor central, DB criptografado, transporte TLS/QUIC
- Binário único cross-platform via Tauri
- ID único gerado após primeira autenticação online no servidor
- Dois tipos senha: uso único (temporária) e permanente (configurada pelo usuário)
- Captura tela hardware-accelerada por plataforma
- Codec H.264/H.265 hardware encoding/decoding

### Non-Goals (Fase 0)
- Áudio remoto
- Múltiplas conexões simultâneas
- Licenciamento/comercialização
- Categorias/tags/dispositivos organizados
- Instaladores customizados por empresa
- Suporte mobile (Android/iOS)
- Chat/arquivos/clipboard sync

### Anti-Goals
- Dependência de código RustDesk ou forks
- Latência > 200ms em condições normais
- Binário > 10MB
- Auth puramente P2P sem validação servidor
- DB servidor em plaintext
- Fallback software encoding como padrão (apenas se GPU indisponível)

## 3. Constraints

- Stack: Rust core + Tauri (WebView frontend)
- Transporte: QUIC/UDP com fallback TCP
- Modelo rede: híbrido P2P + relay VPS fallback
- Captura: DXGI (Windows), PipeWire/XDG Portal (Linux), ScreenCaptureKit (Mac)
- Codec: H.264/H.265 via NVENC/AMF/VAAPI/VideoToolbox; x264 software fallback
- Servidor: relay + sinalização + auth + DB criptografado
- Prioridade plataformas: Windows > Linux > Mac
- Fase 0 escopo: vídeo + input apenas
- Produto comercial futuro mas licenciamento fora do escopo agora

## 4. Arquitetura Técnica

### 4.1 Modelo de Rede

**Híbrido P2P com relay fallback.** Fluxo:

1. Cliente A inicia → conecta ao servidor VPS via QUIC/TLS
2. Servidor valida credenciais, retorna ID único se primeiro acesso
3. Cliente B conecta ao servidor, fornece ID + senha de A
4. Servidor valida senha, troca candidatos ICE entre A e B
5. Tentativa P2P direta via UDP hole punching
6. Se P2P falha (NAT restritivo), relay na VPS assume transparentemente
7. Sessão estabelecida: vídeo + input fluem direto (P2P) ou via relay

**Por que híbrido:** P2P puro dá latência mínima mas falha em ~30% dos casos NAT. Relay garante conectividade universal. Servidor central obrigatório para auth evita conexões não autorizadas mesmo em modo P2P.

### 4.2 Stack Tecnológica

| Componente | Escolha | Motivo |
|------------|---------|--------|
| Core | Rust | Performance, memory safety, cross-platform |
| UI Framework | Tauri v2 | Binário ~3-5MB, WebView nativa, IPC zero-copy |
| Frontend | React + TypeScript + WebGL | Canvas rendering vídeo, input events pass-through |
| Transporte | quinn (QUIC) + tokio | UDP low-latency, multiplexing, TLS 1.3 built-in |
| Captura Windows | DXGI Desktop Duplication | Zero-copy GPU, < 1ms frame capture |
| Captura Linux | PipeWire + XDG Portal | Wayland/X11 compatível, zero-copy DMA-BUF |
| Captura Mac | ScreenCaptureKit | Apple native, hardware accelerated |
| Encoding | H.264/H.265 hardware | NVENC (NVIDIA), AMF (AMD), VAAPI (Intel/Linux), VideoToolbox (Mac) |
| Encoding Fallback | x264 software | Apenas se GPU indisponível |
| Servidor Relay | Rust + quinn + tonic | Mesmo stack cliente, performance consistente |
| DB Servidor | SQLite + SQLCipher | Criptografia AES-256 transparente, leve, embedded |
| Sinalização | gRPC over QUIC | Low-overhead, streaming bidirecional |
| Auth | Argon2id password hashing | Memory-hard, resistente a GPU cracking |

### 4.3 Captura de Tela

**Windows:** DXGI Desktop Duplication API
- Frame capture direto da GPU compositor
- Zero-copy para texture D3D11
- Latência típica: 0.5-2ms
- Fallback: BitBlt/GDI apenas se DXGI falhar (raro)

**Linux:** PipeWire + XDG Desktop Portal
- DMA-BUF zero-copy para GPU
- Compatível Wayland e X11
- Latência típica: 1-3ms
- Fallback: XSHM para X11 legacy

**Mac:** ScreenCaptureKit (macOS 12.3+)
- Hardware accelerated frame capture
- IOSurface zero-copy para Metal
- Latência típica: 1-2ms
- Fallback: CGDisplayStream para macOS < 12.3

### 4.4 Encoding/Decoding

**Encoder priority chain:**
1. Hardware encoder nativo (NVENC/AMF/VAAPI/VideoToolbox)
2. Software x264 ultrafast preset (fallback)

**Configuração codec:**
- Perfil: Baseline/Main (compatibilidade máxima)
- Preset: lowest latency hardware / ultrafast software
- Bitrate adaptativo: 500kbps-8Mbps baseado em bandwidth estimation
- Keyframe interval: 1s ou on-demand (scene change)
- Slice: 1 por frame para baixa latência

**Decoder:** Hardware preferido, libx264 software fallback. Renderização via WebGL texture upload direto.

### 4.5 Input Handling

Mouse e teclado capturados no viewer, enviados via QUIC stream dedicado. Protocolo:

```
InputEvent {
    type: MouseMove | MouseDown | MouseUp | KeyDown | KeyUp | Scroll
    timestamp_us: u64
    data: [u8; N]  // variável por tipo
}
```

Latência input alvo: < 10ms overhead além de rede. Timestamps para jitter compensation. Server-side prediction não aplicável (remote desktop, não game).

### 4.6 Autenticação e Segurança

**Fluxo auth:**

1. Instalação → app gera par chaves Ed25519 localmente
2. Primeira conexão online → envia public key ao servidor
3. Servidor gera ID único (9 dígitos, formato XXX-XXX-XXX), associa à chave
4. Usuário define senha permanente (opcional) ou usa senha uso único gerada pelo app
5. Conexão: viewer envia ID + senha → servidor valida hash Argon2id → se OK, troca candidatos ICE + session token efêmero
6. Session token assinado (Ed25519), expira em 24h, revogável

**DB criptografado:** SQLCipher AES-256-CBC. Chave derivada de master key via HKDF. Master key armazenada em env var ou HSM (futuro). Backups também criptografados.

**Transporte:** QUIC TLS 1.3 obrigatório. Certificados Let's Encrypt para servidor. Pinning opcional em builds futuras.

**Senhas:**
- Uso único: 6 dígitos, validade 1h, regenerável
- Permanente: user-defined, mínimo 8 chars, hash Argon2id (m=64MB, t=3, p=4)
- Futuro: senha por empresa/licença (fora do escopo fase 0)

### 4.7 Estrutura de Diretórios

```
/var/www/html_cntdesk/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── cntdesk-core/       # Lógica compartilhada: protocolo, codec, input
│   ├── cntdesk-client/     # App Tauri (binário principal)
│   ├── cntdesk-server/     # Relay + auth + sinalização
│   └── cntdesk-capture/    # Captura tela platform-specific
├── ui/                     # Frontend React/TypeScript
│   ├── src/
│   ├── public/
│   └── package.json
├── migrations/             # SQL schema migrations
├── docs/                   # Documentação
├── tests/                  # Integration tests
└── scripts/                # Build/deploy helpers
```

## 5. Decisões Técnicas Tomadas

| Decisão | Escolha | Alternativas Rejeitadas | Motivo |
|---------|---------|------------------------|--------|
| Linguagem core | Rust | C++, Go, Zig | Memory safety + performance + ecosystem maduro |
| UI framework | Tauri v2 | egui, Flutter, Electron | Binário pequeno, WebView nativa, cross-platform |
| Transporte | QUIC (quinn) | WebRTC, raw UDP, TCP | Low-latency, multiplexing, TLS built-in, NAT traversal melhor |
| Modelo rede | Híbrido P2P+relay | P2P puro, relay puro | Confiabilidade + latência ótima |
| Captura Windows | DXGI | GDI, WGC | Zero-copy GPU, latência mínima |
| Captura Linux | PipeWire | XSHM, wlr-screencopy | Wayland/X11 universal, zero-copy |
| Captura Mac | ScreenCaptureKit | CGDisplayStream | Hardware accel moderno |
| Codec | H.264/H.265 HW | VP8/VP9/AV1 | Suporte hardware universal, latência menor |
| DB servidor | SQLite+SQLCipher | PostgreSQL, Redis | Embedded, criptografia transparente, leve |
| Auth | ID+senha+Ed25519 | OAuth, mTLS | Simplicidade UX + segurança suficiente |
| Frontend | React+WebGL | Canvas 2D, Three.js | Performance rendering vídeo, ecossistema |

## 6. O Que Fica Para o Builder

- Implementação detalhada dos módulos captura por plataforma
- Tuning fino encoder parameters por GPU específica
- Jitter buffer e adaptive bitrate algorithm
- Error recovery e reconnection logic
- Testes de stress e benchmarking latência
- Packaging/installer por plataforma
- CI/CD pipeline
- Documentação API interna

## 7. Riscos e Mitigações

| Risco | Impacto | Mitigação |
|-------|---------|-----------|
| Hardware encoder indisponível | Latência alta | Fallback x264 ultrafast + warning ao usuário |
| NAT muito restritivo bloqueia P2P | Latência relay | Relay VPS otimizado, múltiplas regiões futuras |
| WebView overhead no frontend | Frame drops | Rendering via WebGL direto, IPC mínimo |
| SQLCipher performance | Auth lento | Connection pooling, cache sessions |
| ScreenCaptureKit requer macOS 12.3+ | Users antigos | Fallback CGDisplayStream documentado |
| QUIC blocked por firewall corporativo | Conectividade | Fallback TCP/TLS automático |

## 8. Métricas de Sucesso (Fase 0)

- Latência end-to-end < 50ms LAN, < 150ms WAN relay
- Frame rate ≥ 30fps sustentado 1080p
- Binário < 10MB
- Tempo conexão < 3s
- Zero crashes em teste 24h contínuo
- Auth success rate > 99.9%
- P2P success rate > 70% (resto via relay)

---

**Próximo passo:** Aprovação deste documento → superpowers:writing-plans → implementação fase 0.