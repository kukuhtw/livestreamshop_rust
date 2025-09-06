# 📺 LiveStreamShop (Rust)

**LiveStreamShop** adalah aplikasi live streaming shopping berbasis web yang bersifat open-source, dibangun dengan **Rust**.
Menawarkan platform mandiri bagi penjual: live stream di situs Anda sendiri, interaksi real-time dengan pembeli melalui chat, serta integrasi keranjang belanja langsung dalam sesi—semua dalam satu alur yang mulus.

---

## Why LiveStreamShop?

Sebelumnya, banyak penjual bergantung pada platform seperti TikTok atau Shopee untuk live commerce. LiveStreamShop hadir sebagai **alternatif mandiri**—penjual tetap pegang kendali penuh atas data, branding, dan interaksi dengan pembeli.

---

## Fitur Utama

* 🎥 Live Streaming langsung dari website.
* Chat real-time untuk interaksi langsung pembeli.
* Keranjang & checkout terintegrasi selama live.
* Kepemilikan data ditangani sepenuhnya oleh penjual.
* Kode open-source—mudah dikustomisasi dan diberi branding.
* Dibangun dengan Rust untuk performa cepat dan aman.

---

## Demo Video

Lihat demo penggunaan LiveStreamShop langsung di video berikut yang menunjukkan alur live streaming dengan chat, interaksi, hingga proses checkout:

[[LiveStreamShop Rust Demo]()](https://www.youtube.com/watch?v=oojtmtgQ1vI)

*(Catatan: jika video gagal memuat, coba akses langsung di YouTube dengan tautan `https://www.youtube.com/watch?v=oojtmtgQ1vI`.)*

---

## Tech Stack

* **Backend**: Rust (Axum, Tokio, SQLx)
* **Frontend**: HTML, CSS, JavaScript
* **Database**: MySQL / PostgreSQL (direncanakan)
* **Video**: WebRTC / Media APIs (direncanakan)

---

## Mulai Cepat

### 1. Clone Repository

```bash
git clone https://github.com/kukuhtw/livestreamshop_rust.git
cd livestreamshop_rust
```

### 2. Setup Environment

Tambahkan file `.env` dengan konfigurasi seperti:

```env
# Port aplikasi (default 3030 jika tidak diisi)
PORT=3030

# Database MySQL
DATABASE_URL=mysql://root:password@127.0.0.1:3306/livestream_shop?ssl-mode=DISABLED

# Session cookie
SESSION_COOKIE_NAME=sid

# Upload directory (lokasi file disimpan di server)
UPLOAD_DIR=../webapp/uploads

# URL publik untuk akses file upload
PUBLIC_BASE_URL=/static/uploads

# Nama aplikasi
APP_NAME="Live Stream Shop"
```

### 3. Jalankan Server

```bash
cargo run
```

Akses server di: [http://127.0.0.1:3030](http://127.0.0.1:3030)

### 4. Buat Admin Pertama

Buka halaman berikut untuk membuat akun admin pertama:
[http://127.0.0.1:3030/static/setupadmin.html](http://127.0.0.1:3030/static/setupadmin.html)

---

## Struktur Proyek

```
livestreamshop_rust/
├── assets/
│   └── haarcascade_frontalface_default.xml  # XML deteksi wajah untuk fitur video di masa depan
│
├── server/
│   ├── src/
│   │   ├── handlers/
│   │   │   ├── admin.rs        # Rute & logika admin
│   │   │   ├── cart.rs         # Logika keranjang belanja
│   │   │   ├── mod.rs          # Modul routing
│   │   │   ├── orders.rs       # Manajemen pesanan
│   │   │   ├── products.rs     # Produk & katalog
│   │   │   └── users.rs        # Autentikasi & profil pengguna
│   │   └── main.rs             # Entrypoint server
│   ├── .env                   # Konfigurasi environment
│   ├── Cargo.toml             # Metadata & dependensi Rust
│   └── Cargo.lock             # Lockfile otomatis
│
├── uploads/                    # Folder penyimpanan file upload (gambar/video)
│
├── webapp/
│   └── uploads/
│       ├── admin.html          # UI dashboard admin
│       ├── index.html          # Halaman utama
│       ├── index.js            # Logika frontend
│       ├── livepage.html       # Halaman live streaming
│       ├── setupadmin.html     # Halaman setup admin pertama
│       └── viewer.html         # Halaman viewer/pembeli
│
├── LICENSE                     # Lisensi (MIT, dsb.)
├── mysignaturee.txt            # Informasi penanda tangan penulis
└── README.md                   # Dokumentasi proyek
```

---

## Kontribusi

Kontribusi sangat disambut!

* Fork repositori ini
* Buat branch fitur
* Submit pull request

---

## Kontak

* **Author**: Kukuh Tripamungkas Wicaksono (Kukuh TW)
* **Email**: [kukuhtw@gmail.com](mailto:kukuhtw@gmail.com)
* **WhatsApp**: [Chat sekarang](https://wa.me/628129893706)
* **LinkedIn**: [Profil](https://id.linkedin.com/in/kukuhtw)

---

## Lisensi

Proyek ini dilisensikan di bawah **Apache 2.0 License**—bebas digunakan, modifikasi, dan disebarkan.

-
