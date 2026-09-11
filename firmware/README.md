# Firmware — Coffee E-Nose (ESP32-S3 + ThingsBoard Cloud, MQTT)

Firmware Rust (`no_std`, esp-hal + embassy) untuk ESP32-S3 yang:

1. Membaca array sensor gas melalui ADC eksternal **ADS1115** (I2C).
2. Konek WiFi dan kirim telemetry ke **ThingsBoard Cloud** via MQTT (dengan
   keep-alive ping, auto-reconnect kalau putus).
3. WiFi & ThingsBoard bisa diganti **tanpa reflash** lewat wizard di serial
   monitor (tekan `c` saat boot), tersimpan di flash internal.

> **Status: sudah pernah di-build, di-flash, dan terbukti kirim data 8 sensor
> ke ThingsBoard di hardware asli.** Fitur **OTA firmware update** (lewat
> ThingsBoard OTA package) baru sebatas deteksi + log, belum aktif menulis ke
> flash — lihat komentar `TODO OTA` di [`src/main.rs`](src/main.rs) untuk
> alasannya (konflik versi `esp-hal` antara `esp-wifi` dan `esp-hal-ota`).
>
> **Soal ganti WiFi:** ada 2 cara, dan keduanya sekarang saling konsisten
> (fix 2026-09-11, lihat [`src/config.rs`](src/config.rs)):
> - **Edit `wifi_config.txt` + build & flash ulang** → otomatis kepakai,
>   tidak perlu wizard lagi (firmware mendeteksi file itu berubah lewat
>   hash, lalu menimpa config lama di flash).
> - **Wizard serial** (tanpa reflash, lihat bagian di bawah) → tetap
>   berguna buat yang tidak punya laptop dev/toolchain (mis. teman yang
>   cuma pegang board-nya). Hasil wizard tetap tersimpan selama
>   `wifi_config.txt` tidak diubah+reflash lagi setelahnya.
>
> Sebelum fix ini, config hasil wizard SELALU menang dibanding
> `wifi_config.txt` selamanya (walau sudah reflash berkali-kali) — itu
> penyebab bingung berjam-jam waktu debugging. Kalau nemu firmware versi
> lama tanpa fix ini, edit file doang tidak akan kelihatan efeknya, wajib
> jalankan wizard.

## Bagian 1 — Setup ThingsBoard Cloud

### 1.1 Buat akun & device

1. Buka [thingsboard.cloud](https://thingsboard.cloud) → daftar/login.
2. Di menu kiri, buka **Entities → Devices** → tombol **+** → **Add new device**.
3. Isi nama device (mis. `coffee-enose`), device profile `default`, klik **Add**.
4. Buka device yang baru dibuat → tab **Details** → klik **Copy access token**.
   Simpan token ini — dipakai di `wifi_config.txt` (Bagian 3) atau wizard serial.

### 1.2 Cek telemetry masuk

Setelah firmware jalan dan konek, cek di device → tab **Latest telemetry**,
harus muncul `gas_ch0`..`gas_ch3` ter-update tiap ±5 detik.

## Bagian 2 — Prasyarat build (sekali saja per laptop)

- Rust toolchain `esp` (via [espup](https://github.com/esp-rs/espup)):
  ```powershell
  cargo install espup
  espup install
  . "$HOME\export-esp.ps1"   # aktifkan environment toolchain esp (tiap sesi shell baru)
  ```
- [`espflash`](https://github.com/esp-rs/espflash) untuk flashing:
  ```powershell
  cargo install espflash
  ```
- VS Code + extension `rust-analyzer` (opsional tapi direkomendasikan untuk editing).

## Wiring

| ESP32-S3 | ADS1115 |
|----------|---------|
| GPIO8    | SDA     |
| GPIO9    | SCL     |
| 3V3      | VDD     |
| GND      | GND     |

Sensor gas (MQ-x / TGS-x dll.) dihubungkan ke channel A0–A3 ADS1115.

## Bagian 3 — Isi WiFi & ThingsBoard, lalu Build & Flash

### Cara 1: edit file langsung (paling gampang, dipakai sekali di awal)

1. Copy [`wifi_config.example.txt`](wifi_config.example.txt) jadi
   `wifi_config.txt` (di folder `firmware/` yang sama).
2. Buka `wifi_config.txt` di VS Code, isi 4 barisnya:
   ```
   SSID=nama_wifi_anda
   PASSWORD=password_wifi_anda
   TB_HOST=thingsboard.cloud
   TB_TOKEN=access_token_dari_langkah_1.1
   ```
3. Save. File ini **sengaja di-`.gitignore`** (tidak ke-commit ke git) supaya
   password/token tidak ikut ter-upload — `build.rs` otomatis membacanya
   saat compile.

### Cara 2: VS Code Task (build + flash sekali klik)

Tekan **Ctrl+Shift+B** di VS Code → otomatis jalankan task
**"Firmware: Build & Flash"** (lihat [`.vscode/tasks.json`](../.vscode/tasks.json)).
Task lain bisa dipilih lewat **Ctrl+Shift+P → "Run Task"**:
`Firmware: Build`, `Firmware: Flash`, `Firmware: Monitor Serial`,
`Firmware: Build & Flash & Monitor`.

### Cara 3: manual lewat terminal

```powershell
$env:PATH = [System.Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [System.Environment]::GetEnvironmentVariable('Path','User')
. "$HOME\export-esp.ps1"
cd firmware
cargo build --release
espflash flash --port COM5 target/xtensa-esp32s3-none-elf/release/coffee-enose-firmware
espflash monitor --port COM5
```

Ganti `COM5` sesuai port board Anda (cek lewat
`Get-PnpDevice -Class Ports -PresentOnly`).

## Ganti WiFi/ThingsBoard tanpa reflash (wizard serial)

1. `espflash monitor --port COM5`, lalu reset/colok ulang board.
2. Dalam 3 detik setelah boot, muncul prompt "Tekan 'c' lalu Enter..." — ketik `c` + Enter.
3. Ikuti 4 prompt (SSID, password, host ThingsBoard, access token).
4. Device otomatis simpan ke flash internal & reboot pakai config baru.

Config hasil wizard ini dipakai terus selama `wifi_config.txt` tidak diubah.
Begitu `wifi_config.txt` diedit dan di-build+flash ulang, nilai barunya
otomatis menang lagi (lihat catatan di bagian atas).

## Output telemetry

Format JSON yang dikirim ke `v1/devices/me/telemetry` (8 sensor gas):

```json
{"mq3": 12345, "mq6": 12000, "mq7": 11800, "mq135": 12500, "tgs2600": 9000, "tgs2602": 8700, "tgs2611": 9200, "tgs2620": 8950}
```

Nilai adalah raw ADC dari 2x ADS1115 (belum dikonversi ke tegangan/ppm). Kalau
ADS1115 belum tersambung, nilainya 0 semua (fallback, bukan error fatal).
Format yang sama juga dicetak ke serial sebagai baris
`DATA,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620` — dipakai
[`tools/record_sample.ps1`](tools/record_sample.ps1) untuk logging lokal.

## Troubleshooting

- **Gagal connect WiFi ("Disconnected") padahal SSID/password sudah benar**:
  paling sering karena **config lama di flash masih dipakai** (lihat
  peringatan di atas) — jalankan wizard ulang, bukan cuma edit
  `wifi_config.txt`. Penyebab lain: router pakai keamanan **WPA/WPA2-TKIP**
  (versi lama) yang kadang tidak didukung ESP32-S3 — cek di pengaturan WiFi
  HP, kalau ada peringatan "keamanan lemah/TKIP", coba jaringan lain (mis.
  hotspot HP, biasanya WPA2-AES). ESP32-S3 juga cuma support **2.4GHz**,
  bukan 5GHz.
- **MQTT connect gagal / device Offline di ThingsBoard**: cek `TB_TOKEN` benar
  (copy-paste dari device details), dan board punya akses internet keluar.
- **`cargo`/`espflash` "not recognized"**: PATH belum ke-refresh, buka
  terminal/VS Code baru, atau jalankan ulang `. "$HOME\export-esp.ps1"`.
- **`espflash monitor` kosong padahal device jalan (data masuk ke ThingsBoard)**:
  `espflash monitor` kadang gagal menangkap output karena proses
  connect/reset-nya sendiri (belum ditelusuri sepenuhnya) — kalau perlu lihat
  log pasti, buka koneksi serial langsung (mis. `System.IO.Ports.SerialPort`
  di PowerShell) setelah reset manual, bukan lewat `espflash monitor`.
