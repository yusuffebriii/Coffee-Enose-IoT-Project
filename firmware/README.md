# Firmware — Coffee E-Nose (ESP32-S3 + ThingsBoard Cloud, MQTT + OTA)

Firmware Rust (`no_std`, esp-hal + embassy) untuk ESP32-S3 yang:

1. Membaca array sensor gas melalui ADC eksternal **ADS1115** (I2C).
2. Konek WiFi dan kirim telemetry ke **ThingsBoard Cloud** via MQTT.
3. Mendukung **OTA firmware update** lewat fitur "OTA Update" ThingsBoard
   (protokol `v2/fw/...`), tulis ke partisi OTA yang tidak aktif, lalu reboot.

> **Status: skeleton, belum pernah di-compile/di-flash ke hardware asli.**
> Ekosistem `esp-hal` (masih versi beta 1.0) dan crate pendukungnya
> (`esp-wifi`, `esp-hal-ota`, `rust-mqtt`) bergerak cepat — cek ulang
> kompatibilitas versi di `Cargo.toml` (`cargo update`, docs.rs) sebelum
> build pertama. Bagian OTA khususnya perlu diverifikasi terhadap API
> `rust-mqtt` yang benar-benar ter-resolve (lihat komentar `TODO` di
> [`src/main.rs`](src/main.rs)) — ini bagian paling berisiko karena salah
> tulis ke flash bisa membuat board tidak bisa boot (perlu re-flash via USB).

## Bagian 1 — Setup ThingsBoard Cloud

### 1.1 Buat akun & device

1. Buka [thingsboard.cloud](https://thingsboard.cloud) → daftar/login.
2. Di menu kiri, buka **Entities → Devices** → tombol **+** → **Add new device**.
3. Isi nama device (mis. `coffee-enose-01`), device profile `default`, klik **Add**.
4. Buka device yang baru dibuat → tab **Details** → klik **Copy access token**.
   Simpan token ini — dipakai sebagai `TB_TOKEN` di firmware (jadi username
   MQTT, tanpa password).

### 1.2 Cek telemetry masuk

Setelah firmware jalan dan konek, cek di device → tab **Latest telemetry**,
harus muncul `gas_ch0`..`gas_ch3` ter-update tiap beberapa detik.

### 1.3 Setup OTA package (untuk update firmware nanti)

1. Menu kiri → **OTA updates** → tab **Firmwares** → **+** → **Upload firmware**.
2. Pilih device profile `default`, isi **Title** & **Version**, pilih file
   `.bin` hasil build (`target/xtensa-esp32s3-none-elf/release/coffee-enose-firmware.bin`,
   perlu di-convert ke image biner via `espflash save-image` — lihat Bagian 3).
3. **Penting:** set **Checksum algorithm** ke **CRC32** — parser firmware ini
   cuma menangani checksum CRC32.
4. Setelah upload, assign firmware ini ke device profile atau device
   spesifik. ThingsBoard otomatis push shared attributes
   (`fw_title`, `fw_version`, `fw_size`, `fw_checksum`, ...) ke device via
   topic `v1/devices/me/attributes` — firmware akan mendeteksi ini dan mulai
   proses OTA otomatis.

## Bagian 2 — Prasyarat build

- Rust toolchain `esp` (via [espup](https://github.com/esp-rs/espup)):
  ```bash
  cargo install espup
  espup install
  source ~/export-esp.sh   # aktifkan environment toolchain esp (tiap sesi shell baru)
  ```
- [`espflash`](https://github.com/esp-rs/espflash) untuk flashing:
  ```bash
  cargo install espflash
  ```

## Wiring (default di skeleton, sesuaikan bila berbeda)

| ESP32-S3 | ADS1115 |
|----------|---------|
| GPIO8    | SDA     |
| GPIO9    | SCL     |
| 3V3      | VDD     |
| GND      | GND     |

Sensor gas (MQ-x / TGS-x dll.) dihubungkan ke channel A0–A3 ADS1115.

Partisi flash pakai layout OTA 2-slot, lihat [`partitions.csv`](partitions.csv)
(cocok untuk board flash 4MB — sesuaikan ukuran kalau flash board Anda beda).

## Bagian 3 — Build & Flash

Set environment variable dulu (WiFi + ThingsBoard):

```bash
export SSID=NamaWifiAnda
export PASSWORD=PasswordWifiAnda
export TB_HOST=thingsboard.cloud
export TB_TOKEN=<access_token_dari_langkah_1.1>
```

PowerShell:

```powershell
$env:SSID="NamaWifiAnda"
$env:PASSWORD="PasswordWifiAnda"
$env:TB_HOST="thingsboard.cloud"
$env:TB_TOKEN="<access_token_dari_langkah_1.1>"
```

Build + flash + monitor (flash pertama kali wajib via USB):

```bash
cargo build --release
cargo run --release
```

Untuk siapkan file OTA (upload ke ThingsBoard, Bagian 1.3):

```bash
espflash save-image --chip esp32s3 target/xtensa-esp32s3-none-elf/release/coffee-enose-firmware coffee-enose-firmware.bin
```

## Output telemetry

Format JSON yang dikirim ke `v1/devices/me/telemetry`:

```json
{"gas_ch0": 12345, "gas_ch1": 12000, "gas_ch2": 11800, "gas_ch3": 12500}
```

Nilai adalah raw ADC dari ADS1115 (belum dikonversi ke tegangan/ppm). Sesuaikan
dengan skema yang diharapkan pipeline di [`../01_raw_data/`](../01_raw_data/).

## Troubleshooting

- **Gagal connect WiFi**: cek SSID/PASSWORD, ESP32-S3 hanya support WiFi 2.4GHz.
- **MQTT connect gagal**: cek `TB_TOKEN` benar (copy-paste dari device details),
  dan `TB_HOST` bisa di-resolve (device harus punya akses internet keluar).
- **OTA gagal terus**: cek checksum algorithm di ThingsBoard sudah CRC32,
  bukan SHA256 (default-nya kadang SHA256, harus diganti manual).
- **Board tidak mau boot setelah OTA**: flash ulang via USB dengan
  `cargo run --release` (partisi `ota_0` yang lama masih ada, bootloader bisa
  di-reset via `espflash erase-flash` lalu flash ulang dari awal kalau perlu).
