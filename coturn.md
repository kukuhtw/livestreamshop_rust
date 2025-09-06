

# 🧊 TURN Server (coturn) – Contoh Konfigurasi

## 1) Instalasi & User

Debian/Ubuntu:

```bash
sudo apt update
sudo apt install coturn -y
```

Buat user kredensial (long-term credentials):

```bash
sudo turnadmin -a -u livestream -p superSecret123 -r yourdomain.com
# -a: add, -u: username, -p: password, -r: realm
```

> Kamu bisa simpan user di file juga (lihat opsi `userdb` di manual), tapi untuk cepat, pakai perintah di atas.

---

## 2) `turnserver.conf` (TLS + long-term credentials)

Letakkan di `/etc/turnserver.conf`:

```ini
# ===== Listening & Relay =====
listening-port=3478
tls-listening-port=5349
listening-ip=0.0.0.0
relay-ip=<PUBLIC_IP>              # IP publik server
external-ip=<PUBLIC_IP>           # Jika server punya NAT di depan, isi dengan "PUBLIC_IP/PRIVATE_IP"
min-port=49160
max-port=49200                    # Rentang port relay (buka di firewall)

# ===== Identity & Auth =====
realm=yourdomain.com
fingerprint
lt-cred-mech                      # Long-Term Credentials (username+password)
# User dibuat via `turnadmin`; bisa juga pakai:
# user=livestream:superSecret123  # (opsional quick test)

# ===== Certificates (TLS) =====
cert=/etc/letsencrypt/live/yourdomain.com/fullchain.pem
pkey=/etc/letsencrypt/live/yourdomain.com/privkey.pem
no-sslv3
no-tlsv1
no-tlsv1_1

# ===== Security / QoS =====
stale-nonce=600
no-loopback-peers
no-multicast-peers
denied-peer-ip=10.0.0.0-10.255.255.255
denied-peer-ip=192.168.0.0-192.168.255.255
denied-peer-ip=172.16.0.0-172.31.255.255
# (hapus deny di atas kalau kamu butuh akses private network)
pidfile=/var/run/turnserver.pid
simple-log
log-file=/var/log/turnserver/turn.log

# ===== Performance =====
no-cli
no-tcp-relay                      # bisa aktifkan jika memang perlu
user-quota=12
total-quota=120
```

> Ganti `yourdomain.com` dan `<PUBLIC_IP>` sesuai server kamu.

---

## 3) Firewall

Buka port berikut di firewall:

* UDP/TCP **3478** (STUN/TURN plaintext)
* UDP/TCP **5349** (TURN TLS)
* UDP **49160-49200** (relay RTP/RTCP – sesuaikan dengan config)

Contoh `ufw`:

```bash
sudo ufw allow 3478/tcp
sudo ufw allow 3478/udp
sudo ufw allow 5349/tcp
sudo ufw allow 5349/udp
sudo ufw allow 49160:49200/udp
```

---

## 4) Systemd

Aktifkan service:

```bash
# Pastikan konfigurasi OK
sudo turnserver -n -c /etc/turnserver.conf -o
# Jalankan & enable
sudo systemctl enable coturn
sudo systemctl restart coturn
sudo systemctl status coturn
```

Log:

```bash
sudo tail -f /var/log/turnserver/turn.log
```

---

## 5) Konfigurasi Client (LiveStreamShop)

Di `webrtc.js`, set `ICE` seperti ini:

```js
const ICE = [
  { urls: 'stun:stun.l.google.com:19302' },            // optional STUN publik
  {
    urls: [
      'turns:turn.yourdomain.com:5349',                // TLS preferred
      'turn:turn.yourdomain.com:3478'                  // fallback
    ],
    username: 'livestream',
    credential: 'superSecret123'
  }
];
```

> **Gunakan `turns:` (TLS/5349)** bila memungkinkan. Pastikan sertifikat valid (Let’s Encrypt, dsb).

---

## 6) Tes Dasar

**Tes port TLS 5349**:

```bash
openssl s_client -connect turn.yourdomain.com:5349 -quiet
```

Jika handshake TLS berhasil (tampil sertifikat), port TLS OK.

**Tes from browser**: jalankan app kamu, lalu cek di **Developer Tools → Network → WebRTC** (Chrome) / **about\:webrtc** (Firefox) untuk melihat ICE candidates. Jika kamu melihat `relay` candidates (typ relay), TURN berfungsi.

---

## 7) Docker (Alternatif)

`docker-compose.yml` contoh:

```yaml
version: "3.8"
services:
  coturn:
    image: instrumentisto/coturn
    restart: unless-stopped
    network_mode: host           # supaya mudah expose UDP relay
    volumes:
      - ./turnserver.conf:/etc/coturn/turnserver.conf:ro
      - /etc/letsencrypt:/etc/letsencrypt:ro
    command: ["-c", "/etc/coturn/turnserver.conf", "-o"]
```

> `network_mode: host` memudahkan UDP relay. Jika tidak host-mode, pastikan map port UDP relay range dengan benar.

---

## 8) Tips & Troubleshooting

* **Autentikasi gagal**: pastikan `realm` di `turnserver.conf` sama dengan yang kamu pakai saat membuat user dan **client** (tidak perlu set realm di client, tapi user dibuat dengan realm itu).
* **Tidak muncul relay candidate**: cek firewall UDP **49160–49200**, pastikan traffic UDP tidak di-drop upstream (VPS/Cloud).
* **TLS error**: pastikan sertifikat domain benar, jam server benar (NTP), dan gunakan `turns:`.
* **Koneksi masih `host/srflx` saja**: jaringan mungkin tidak butuh TURN; bagus. TURN akan dipakai hanya jika diperlukan.
* **Multiple NIC / NAT**: gunakan `external-ip=PUBLIC/PRIVATE` format jika server di belakang NAT (lihat `man turnserver`).
* **Bandwidth**: TURN mem-forward media → perhatikan biaya trafik. Untuk banyak viewer, pertimbangkan **SFU**.
