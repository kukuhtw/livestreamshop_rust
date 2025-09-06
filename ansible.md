
# 📦 Struktur Project (disarankan)

```
infra/
├── inventory.ini
├── group_vars/
│   └── turn.yml
├── playbooks/
│   └── coturn.yml
└── roles/
    └── coturn/
        ├── tasks/main.yml
        ├── templates/turnserver.conf.j2
        └── handlers/main.yml
```

---

## 1) `inventory.ini`

```ini
[turn]
turn1 ansible_host=YOUR_SERVER_IP ansible_user=ubuntu   # ganti user sesuai OS image
```

> Pastikan SSH key sudah terpasang di server.

---

## 2) `group_vars/turn.yml` (variabel utama)

```yaml
turn_domain: "turn.yourdomain.com"
turn_realm: "yourdomain.com"
turn_public_ip: "YOUR_PUBLIC_IP"     # contoh: "203.0.113.10"
turn_private_ip: ""                   # optional; jika di belakang NAT, isi "203.0.113.10/10.0.0.5"
turn_user: "livestream"
turn_pass: "superSecret123"           # simpan pakai ansible-vault di real use

turn_min_port: 49160
turn_max_port: 49200

# OS family: debian (Ubuntu/Debian)
turn_packages:
  - coturn
  - ufw
  - certbot
  - python3-certbot-nginx   # boleh, walau kita tak perlu Nginx untuk coturn TLS
                           # certbot standalone dipakai saat obtain cert

# UFW enable? (set true kalau pakai UFW)
turn_enable_ufw: true
```

> Rekomendasi: **encrypt** `turn_pass` dengan `ansible-vault`.

---

## 3) `playbooks/coturn.yml`

```yaml
---
- name: Provision TURN (coturn)
  hosts: turn
  become: true
  vars_files:
    - ../group_vars/turn.yml

  roles:
    - coturn
```

---

## 4) `roles/coturn/tasks/main.yml`

```yaml
---
- name: Update apt cache
  ansible.builtin.apt:
    update_cache: true
  when: ansible_os_family == "Debian"

- name: Install packages
  ansible.builtin.apt:
    name: "{{ turn_packages }}"
    state: present
  when: ansible_os_family == "Debian"

- name: Ensure coturn log dir exists
  ansible.builtin.file:
    path: /var/log/turnserver
    state: directory
    owner: turnserver
    group: turnserver
    mode: "0755"

- name: Obtain/renew Let’s Encrypt cert (standalone)
  community.crypto.acme_certificate:
    account_key_src: /etc/letsencrypt/accounts/{{ turn_domain }}.key
    acme_directory: https://acme-v02.api.letsencrypt.org/directory
    certificate_key_src: /etc/letsencrypt/live/{{ turn_domain }}/privkey.pem
    fullchain_dest: /etc/letsencrypt/live/{{ turn_domain }}/fullchain.pem
    private_key_dest: /etc/letsencrypt/live/{{ turn_domain }}/privkey.pem
    csr: |
      -----BEGIN CERTIFICATE REQUEST-----
      # dummy; kita pakai certbot CLI agar lebih simpel pada host berbasis UFW
      # langkah berikut pakai command module.
      -----END CERTIFICATE REQUEST-----
  ignore_errors: true
  changed_when: false

# Simpler: pakai certbot CLI (standalone HTTP-01), pastikan port 80 open sementara
- name: Stop services binding :80 (optional)
  ansible.builtin.service:
    name: nginx
    state: stopped
  ignore_errors: true

- name: Ensure cert directory exists
  ansible.builtin.file:
    path: /etc/letsencrypt/live/{{ turn_domain }}
    state: directory
    mode: "0755"

- name: Obtain TLS certificate via certbot (standalone)
  ansible.builtin.command:
    cmd: >
      certbot certonly --standalone --non-interactive --agree-tos
      -m admin@{{ turn_realm }}
      -d {{ turn_domain }}
  args:
    creates: "/etc/letsencrypt/live/{{ turn_domain }}/fullchain.pem"

- name: Start nginx back (if was present)
  ansible.builtin.service:
    name: nginx
    state: started
  ignore_errors: true

- name: Render turnserver.conf
  ansible.builtin.template:
    src: turnserver.conf.j2
    dest: /etc/turnserver.conf
    owner: root
    group: root
    mode: "0644"
  notify:
    - Restart coturn

# Buat user long-term credentials
- name: Create TURN user (lt-cred-mech)
  ansible.builtin.command:
    cmd: turnadmin -a -u {{ turn_user }} -p {{ turn_pass }} -r {{ turn_realm }}
  register: turnadmin_add
  failed_when: turnadmin_add.rc not in [0,1]
  changed_when: "'already' not in turnadmin_add.stderr|default('') and turnadmin_add.rc == 0"

# Enable coturn service on boot
- name: Enable coturn at boot
  ansible.builtin.systemd:
    name: coturn
    enabled: true

# UFW
- name: Enable UFW (if requested)
  ansible.builtin.ufw:
    state: enabled
  when: turn_enable_ufw

- name: Allow UFW ports (STUN/TURN)
  ansible.builtin.ufw:
    rule: allow
    port: "{{ item.port }}"
    proto: "{{ item.proto }}"
  loop:
    - { port: "3478", proto: "tcp" }
    - { port: "3478", proto: "udp" }
    - { port: "5349", proto: "tcp" }
    - { port: "5349", proto: "udp" }
  when: turn_enable_ufw

- name: Allow UFW relay UDP range
  ansible.builtin.ufw:
    rule: allow
    port: "{{ turn_min_port }}:{{ turn_max_port }}"
    proto: udp
  when: turn_enable_ufw

# Cron renew (certbot handles systemd timer by default, but add hook to restart coturn)
- name: Install renew hook to restart coturn
  ansible.builtin.copy:
    dest: /etc/letsencrypt/renewal-hooks/deploy/99-restart-coturn.sh
    mode: "0755"
    content: |
      #!/usr/bin/env bash
      systemctl restart coturn || true
```

> Catatan: Banyak distro sudah memasang **systemd timer** `certbot.timer`. Hook di atas memastikan **coturn restart** setelah cert diperbarui.

---

## 5) `roles/coturn/templates/turnserver.conf.j2`

```jinja
# ===== Listening & Relay =====
listening-port=3478
tls-listening-port=5349
listening-ip=0.0.0.0
relay-ip={{ turn_public_ip }}
{% if turn_private_ip|length > 0 %}
external-ip={{ turn_private_ip }}
{% else %}
external-ip={{ turn_public_ip }}
{% endif %}
min-port={{ turn_min_port }}
max-port={{ turn_max_port }}

# ===== Identity & Auth =====
realm={{ turn_realm }}
fingerprint
lt-cred-mech

# ===== Certificates (TLS) =====
cert=/etc/letsencrypt/live/{{ turn_domain }}/fullchain.pem
pkey=/etc/letsencrypt/live/{{ turn_domain }}/privkey.pem
no-sslv3
no-tlsv1
no-tlsv1_1

# ===== Security / QoS =====
stale-nonce=600
no-loopback-peers
no-multicast-peers
# Deny private ranges by default (hapus jika butuh akses private LAN)
denied-peer-ip=10.0.0.0-10.255.255.255
denied-peer-ip=192.168.0.0-192.168.255.255
denied-peer-ip=172.16.0.0-172.31.255.255

pidfile=/var/run/turnserver.pid
simple-log
log-file=/var/log/turnserver/turn.log

# ===== Performance =====
no-cli
# no-tcp-relay        # uncomment jika ingin disable TCP relay
user-quota=12
total-quota=120
```

---

## 6) `roles/coturn/handlers/main.yml`

```yaml
---
- name: Restart coturn
  ansible.builtin.service:
    name: coturn
    state: restarted
```

---

## 7) Jalankan

```bash
cd infra
ansible-playbook -i inventory.ini playbooks/coturn.yml
```

Kalau pakai **ansible-vault** untuk `turn_pass`:

```bash
ansible-vault create group_vars/turn.yml
# paste isi variabel (atau at least turn_pass) lalu simpan

ansible-playbook -i inventory.ini playbooks/coturn.yml --ask-vault-pass
```

---

## 8) Konfigurasi di LiveStreamShop (`webrtc.js`)

```js
const ICE = [
  { urls: 'stun:stun.l.google.com:19302' },
  {
    urls: [
      'turns:{{ turn_domain }}:5349', // TLS
      'turn:{{ turn_domain }}:3478'   // plaintext fallback
    ],
    username: '{{ turn_user }}',
    credential: '{{ turn_pass }}'
  }
];
```

---

## 9) Verifikasi & Debug

* TLS: `openssl s_client -connect {{ turn_domain }}:5349 -quiet`
* Log: `sudo journalctl -u coturn -f` dan `sudo tail -f /var/log/turnserver/turn.log`
* Browser: Chrome → `chrome://webrtc-internals` / Firefox → `about:webrtc`

  * Pastikan muncul **relay** candidates saat NAT ketat.
* Firewall/cloud: pastikan UDP range `{{turn_min_port}}–{{turn_max_port}}` tidak di-drop di sisi provider.

