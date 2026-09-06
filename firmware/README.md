# Firmware — Coffee E-Nose (ESP32-S3 + ThingsBoard Cloud, MQTT)

Firmware Rust (`no_std`, esp-hal + embassy) untuk ESP32-S3 yang:

1. Membaca array sensor gas melalui ADC eksternal **ADS1115** (I2C).
2. Konek WiFi dan kirim telemetry ke **ThingsBoard Cloud** via MQTT (dengan
   keep-alive ping, auto-reconnect kalau putus).
3. WiFi & ThingsBoard bisa diganti **tanpa reflash** lewat wizard di serial
   monitor (tekan `c` saat boot), tersimpan di flash internal.

> **Status: sudah pernah di-build, di-flash, dan terbukti kirim data ke
> ThingsBoard di hardware asli.** Fitur **OTA firmware update** (lewat
> ThingsBoard OTA package) baru sebatas deteksi + log, belum aktif menulis ke
> flash — lihat komentar `TODO OTA` di [`src/main.rs`](src/main.rs) untuk
> alasannya (konflik versi `esp-hal` antara `esp-wifi` dan `esp-hal-ota`).
> Log lewat macro `log::info!`/`error!` juga belum kelihatan di serial
> monitor (beda dengan `esp_println::println!` yang normal) — belum
> ditelusuri penyebabnya, tapi tidak mempengaruhi fungsi utama.

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

Config yang tersimpan di flash ini **override** nilai di `wifi_config.txt`.
Kalau mau reset ke nilai file lagi, jalankan wizard ulang dengan isian yang
sama seperti file, atau `espflash erase-flash` (lalu flash ulang dari awal).

## Output telemetry

Format JSON yang dikirim ke `v1/devices/me/telemetry`:

```json
{"gas_ch0": 12345, "gas_ch1": 12000, "gas_ch2": 11800, "gas_ch3": 12500}
```

Nilai adalah raw ADC dari ADS1115 (belum dikonversi ke tegangan/ppm). Kalau
ADS1115 belum tersambung, nilainya 0 semua (fallback, bukan error fatal).

## Troubleshooting

- **Gagal connect WiFi**: cek SSID/PASSWORD di `wifi_config.txt` (atau config
  hasil wizard), ESP32-S3 hanya support WiFi 2.4GHz.
- **MQTT connect gagal / device Offline di ThingsBoard**: cek `TB_TOKEN` benar
  (copy-paste dari device details), dan board punya akses internet keluar.
- **`cargo`/`espflash` "not recognized"**: PATH belum ke-refresh, buka
  terminal/VS Code baru, atau jalankan ulang `. "$HOME\export-esp.ps1"`.
- **Serial monitor kosong padahal device jalan (data masuk ke ThingsBoard)**:
  known issue, lihat catatan status di atas — bukan berarti firmware crash.
