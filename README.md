
# 📺 LiveStreamShop (Rust)

**LiveStreamShop** adalah aplikasi **live streaming shopping** berbasis web yang bersifat **open-source**, dibangun dengan **Rust**.
Aplikasi ini memungkinkan penjual melakukan **live streaming langsung dari website sendiri**, berinteraksi dengan pembeli via **chat real-time**, dan menyelesaikan transaksi lewat **keranjang belanja terintegrasi**.

---

## 🚀 Mengapa LiveStreamShop?

Mayoritas penjual online harus bergantung pada platform besar seperti TikTok atau Shopee untuk live commerce.
LiveStreamShop hadir sebagai **alternatif mandiri**:

* Kontrol penuh atas **data pembeli**
* Branding bisa dikustomisasi sesuai bisnis
* Tidak terikat aturan & komisi platform lain

---

## ✨ Fitur Utama

* 🎥 **Live Streaming** langsung dari situs Anda
* 💬 **Chat real-time** host ↔ pembeli
* 🛒 **Keranjang & checkout** langsung di sesi live
* 🔐 **Kepemilikan penuh data** (tidak dikunci vendor)
* 🛠️ **Open-source**, bisa dikustomisasi
* ⚡ Dibangun dengan **Rust (Axum, Tokio, SQLx)** → performa cepat & aman

---

## 📼 Demo Video

[![Demo YouTube](https://img.youtube.com/vi/oojtmtgQ1vI/0.jpg)](https://www.youtube.com/watch?v=oojtmtgQ1vI)
👉 Klik thumbnail atau buka langsung: [https://www.youtube.com/watch?v=oojtmtgQ1vI](https://www.youtube.com/watch?v=oojtmtgQ1vI)

---

## 🧩 Tech Stack

* **Backend**: Rust (Axum, Tokio, SQLx)
* **Frontend**: HTML, CSS, JavaScript
* **Database**: MySQL (PostgreSQL planned)
* **Streaming**: HTML5 Video + Canvas (output filter) + **WebRTC** (P2P)
* **Signaling**: WebSocket via Axum

---

## 🔌 Cara Kerja WebRTC

LiveStreamShop memanfaatkan **WebRTC** agar video & audio host bisa langsung dikirim ke viewer tanpa server media khusus.

### 1. Media (Browser)

* Host ambil video dari `<canvas>` + audio dari `getUserMedia()`.
* `RTCPeerConnection` membuat **SDP offer/answer** & ICE gathering.

### 2. STUN/TURN

* Default: **STUN publik** `stun:stun.l.google.com:19302`.
* Untuk NAT ketat → jalankan **coturn** sebagai TURN server terpisah.

### 3. Signaling (Server Rust)

* Server Axum hanya relay JSON lewat WebSocket `/ws/:room`.
* Contoh pesan:

  ```jsonc
  { "t": "offer", "sdp": "..." }
  { "t": "answer", "sdp": "..." }
  { "t": "ice", "candidate": { ... } }
  { "t": "sys", "text": "viewer_enter" }
  ```

### 4. DataChannel (Chat)

* Host → Viewer pakai DataChannel `chat`.
* Fallback: WS `/ws/_events`.

### 5. Autoplay & Audio

* Viewer default **muted** karena aturan browser.
* Harus ada tombol **Unmute** supaya audio jalan.

---

## 🔄 Sequence Diagram WebRTC

```mermaid
sequenceDiagram
    participant H as Host (Browser)
    participant S as Server (Axum /ws/:room)
    participant V as Viewer (Browser)

    H->>H: getUserMedia() / captureStream()
    H->>H: createOffer (SDP)
    H->>S: send { t:"offer", sdp:... }
    S-->>V: relay { t:"offer", sdp:... }

    V->>V: setRemoteDescription(offer)
    V->>V: createAnswer (SDP)
    V->>S: send { t:"answer", sdp:... }
    S-->>H: relay { t:"answer", sdp:... }

    H->>S: send { t:"ice", candidate:... }
    S-->>V: relay { t:"ice", candidate:... }

    V->>S: send { t:"ice", candidate:... }
    S-->>H: relay { t:"ice", candidate:... }

    H-->>V: Media stream (video+audio) via P2P
    V-->>H: DataChannel (chat)
```

---

## 🏗️ Arsitektur Komponen

```mermaid
flowchart TD
    subgraph CLIENT["🌐 Frontend (Browser)"]
      H["Host<br/><small>admin.html + webrtc.js</small>"]
      V["Viewer<br/><small>livepage.html</small>"]
    end

    subgraph SERVER["🦀 Backend (Rust / Axum)"]
      AX["Axum Router<br/><small>HTTP + WebSocket</small>"]
      SF["Static Files<br/><small>tower-http serve /static</small>"]
      API["REST Endpoints<br/><small>/api/* (products, orders, users)</small>"]
      WS["Signaling WS<br/><small>/ws/:room</small>"]
      EVT["Global Events WS<br/><small>/ws/_events</small>"]
    end

    subgraph DATA["💾 Data & Storage"]
      DB["MySQL<br/><small>SQLx</small>"]
      UP["Uploads<br/><small>PUBLIC_BASE_URL</small>"]
    end

    STUN["STUN Server<br/><small>stun:stun.l.google.com:19302</small>"]
    TURN["TURN Server (opsional)<br/><small>coturn</small>"]
    CDN["CDN (opsional)<br/><small>cache static/uploads</small>"]

    %% Static & Pages
    H <-- HTTP GET --> SF
    V <-- HTTP GET --> SF
    SF --- AX

    %% REST API
    H <-- HTTP/JSON --> API
    V <-- HTTP/JSON --> API
    API --- DB
    API --- UP

    %% Signaling
    H <-- WS JSON --> WS
    V <-- WS JSON --> WS
    WS --- AX

    %% Event bus
    H <-- WS JSON --> EVT
    EVT --- AX

    %% WebRTC Plane
    H <-- P2P Media (SRTP) --> V

    %% ICE infra
    H -. ICE .-> STUN
    V -. ICE .-> STUN
    H -. relay .-> TURN
    V -. relay .-> TURN

    %% Optional CDN
    SF -. cache .-> CDN
    UP -. cache .-> CDN
```

---

## 📉 Keterbatasan Versi Saat Ini

* P2P cocok untuk **viewer terbatas** (10–20).
* Ribuan viewer → butuh **SFU/MCU** (mis. mediasoup, Janus, ion-sfu).
* Fitur multi-host, pembayaran otomatis, CDN masih dalam roadmap.

---

## 🛠️ Setup Cepat

### 1. Clone

```bash
git clone https://github.com/kukuhtw/livestreamshop_rust.git
cd livestreamshop_rust
```

### 2. Buat `.env`

```env
PORT=3030
DATABASE_URL=mysql://root:password@127.0.0.1:3306/livestream_shop?ssl-mode=DISABLED
SESSION_COOKIE_NAME=sid
UPLOAD_DIR=../webapp/uploads
PUBLIC_BASE_URL=/static/uploads
APP_NAME="Live Stream Shop"
```

### 3. Jalankan Server

```bash
cargo run
```

Buka: [http://127.0.0.1:3030](http://127.0.0.1:3030)

### 4. Setup Admin

Akses: [http://127.0.0.1:3030/static/setupadmin.html](http://127.0.0.1:3030/static/setupadmin.html)

---

## 📂 Struktur Proyek

```
livestreamshop_rust/
├── server/
│   ├── src/
│   │   ├── handlers/   # routes admin, products, orders, users
│   │   └── main.rs
│   ├── Cargo.toml
│   └── .env
├── webapp/
│   ├── admin.html      # Dashboard admin
│   ├── index.html      # Halaman utama
│   ├── livepage.html   # Halaman viewer live
│   └── webrtc.js       # Logika WebRTC host
└── uploads/            # Penyimpanan file
```

---

## 🤝 Kontribusi

* Fork repo → buat branch → PR
* Diskusi fitur di GitHub Issues

---

## 📬 Kontak

* **Author**: Kukuh Tripamungkas Wicaksono
* **Email**: [kukuhtw@gmail.com](mailto:kukuhtw@gmail.com)
* **WhatsApp**: [wa.me/628129893706](https://wa.me/628129893706)
* **LinkedIn**: [linkedin.com/in/kukuhtw](https://id.linkedin.com/in/kukuhtw)

---

## 📜 Lisensi

Apache 2.0 License – bebas digunakan, dimodifikasi, dan disebarkan.

