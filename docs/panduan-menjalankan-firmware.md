# Panduan Manual: ESP32-S3 → ThingsBoard Cloud (Coffee E-Nose)

Dokumen ini menjelaskan **semua langkah dari nol** untuk membangun (build), meng-upload
(flash), dan memverifikasi firmware Coffee E-Nose berjalan dan mengirim data ke
ThingsBoard Cloud lewat WiFi + MQTT. Ini adalah urutan persis yang dipakai untuk
memvalidasi project ini, jadi bisa diulang sendiri atau dijelaskan ke dosen tanpa ada
langkah yang terlewat.

## 0. Gambaran Besar (untuk dijelaskan ke dosen)

Alurnya:

```
[ADS1115 + sensor gas] --I2C--> [ESP32-S3, firmware Rust] --WiFi--> [Internet] --MQTT--> [ThingsBoard Cloud]
```

1. ESP32-S3 menjalankan firmware yang ditulis dalam bahasa Rust (bare-metal, `no_std`,
   tanpa sistem operasi — semua akses hardware langsung lewat register/HAL).
2. Firmware terhubung ke WiFi rumah/kampus (mode Station/STA).
3. Firmware terhubung ke broker MQTT milik ThingsBoard Cloud (`thingsboard.cloud`,
   port 1883), otentikasi memakai **access token** device sebagai username MQTT.
4. Setiap 5 detik, firmware membaca 4 channel ADC dari ADS1115 (rencananya masing-masing
   channel ke satu sensor gas) dan mengirim datanya sebagai JSON ke topic
   `v1/devices/me/telemetry`.
5. ThingsBoard menyimpan data itu dan menampilkannya di dashboard.

## 1. Persiapan Hardware

- Board ESP32-S3 (terhubung ke laptop lewat kabel USB — pada kasus ini terdeteksi
  sebagai **COM5**, chip USB-serial-nya CH343).
- Modul ADC ADS1115 (I2C, 4 channel single-ended).
- Sensor gas (MQ-x / TGS-x, dst.) terhubung ke channel A0–A3 ADS1115.
- Wiring:

  | ESP32-S3 | ADS1115 |
  |----------|---------|
  | GPIO8    | SDA     |
  | GPIO9    | SCL     |
  | 3V3      | VDD     |
  | GND      | GND     |

  > Catatan: pada saat validasi awal ini, ADS1115 **belum dipasang** — firmware
  > tetap berhasil connect WiFi + MQTT + kirim telemetry, hanya saja nilai
  > `gas_ch0..gas_ch3` semuanya 0 (karena pembacaan I2C gagal dan kode fallback ke 0).
  > Setelah ADS1115 dipasang, nilai ini akan berubah sesuai bacaan sensor asli.

## 2. Setup Akun & Device di ThingsBoard Cloud

1. Buka [thingsboard.cloud](https://thingsboard.cloud), daftar/login.
2. Menu kiri → **Entities → Devices** → tombol **+** → **Add new device**.
3. Isi nama device (mis. `coffee-enose`), Device profile: `default` → **Add**.
4. Buka device tersebut → tab **Details** → klik **Copy access token**. Simpan
   token ini (dipakai sebagai `TB_TOKEN` di langkah build nanti).
5. (Opsional) Buka menu **Dashboards**, buat dashboard baru untuk visualisasi
   `gas_ch0`..`gas_ch3`.

## 3. Install Toolchain di Laptop (sekali saja)

Semua perintah di bawah dijalankan di **PowerShell**.

### 3.1 Install Rust (rustup)

```powershell
winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
```

Setelah selesai, **tutup dan buka ulang terminal** (atau refresh PATH) supaya
`cargo`/`rustup` bisa dipanggil. Verifikasi:

```powershell
cargo --version
rustc --version
```

### 3.2 Install `espup` (installer toolchain Rust khusus chip Espressif/Xtensa)

```powershell
cargo install espup --locked
```

Ini meng-compile dari source, memakan waktu ±5 menit.

### 3.3 Install toolchain Xtensa untuk ESP32-S3

```powershell
espup install
```

Ini mengunduh Rust compiler fork Xtensa + LLVM + GCC (`xtensa-esp-elf`) khusus untuk
chip Espressif. Ukurannya besar, bisa 10–15 menit. Setelah selesai, espup akan
membuat file `export-esp.ps1` di folder home (`C:\Users\<user>\export-esp.ps1`)
yang berisi environment variable yang dibutuhkan toolchain ini (path ke GCC, dst).

**Setiap kali membuka terminal baru sebelum build**, wajib jalankan:

```powershell
. "C:\Users\<user>\export-esp.ps1"
```

(tanda titik + spasi di depan itu wajib, artinya "source" file tersebut ke sesi
shell yang sedang aktif.)

### 3.4 Install `espflash` (tool untuk upload firmware & serial monitor)

```powershell
cargo install espflash --locked
```

## 4. Menyiapkan Environment Variable

Firmware butuh 4 informasi rahasia yang **tidak ditulis di source code**
(supaya tidak ke-commit ke git), tapi di-inject saat compile lewat environment
variable:

```powershell
$env:SSID = "nama_wifi_anda"
$env:PASSWORD = "password_wifi_anda"
$env:TB_HOST = "thingsboard.cloud"
$env:TB_TOKEN = "access_token_dari_langkah_2.4"
```

> WiFi **wajib** band 2.4GHz — ESP32-S3 tidak mendukung WiFi 5GHz.
> Variable ini hanya berlaku untuk sesi PowerShell yang sedang aktif; kalau buka
> terminal baru harus di-set ulang.

## 5. Build Firmware

```powershell
cd "PROJECT_COFFEE_ENOSE\firmware"
cargo build --release
```

Yang terjadi di balik layar:
- Cargo mengunduh & meng-compile semua dependency Rust (esp-hal, esp-wifi,
  embassy, rust-mqtt, ads1x1x, dst).
- Target compile adalah `xtensa-esp32s3-none-elf` (didefinisikan di
  `firmware/.cargo/config.toml`) — artinya biner yang dihasilkan BUKAN untuk
  jalan di Windows, tapi untuk CPU Xtensa di dalam chip ESP32-S3.
- Karena ini `no_std` (tidak pakai OS), bagian `core` dan `alloc` dari standard
  library Rust ikut di-compile ulang dari source khusus untuk target ini
  (diatur lewat `build-std = ["core", "alloc"]`).
- Hasil akhirnya ada di:
  `firmware/target/xtensa-esp32s3-none-elf/release/coffee-enose-firmware`
  (file ELF, belum berupa `.bin` biasa).

Build pertama kali bisa ±2 menit (banyak dependency baru); build berikutnya
jauh lebih cepat karena sudah ke-cache.

## 6. Flash (Upload) Firmware ke ESP32-S3

Pastikan board sudah tersambung USB dan terdeteksi sebagai COM port tertentu.
Cek port yang tersedia:

```powershell
Get-PnpDevice -Class Ports -PresentOnly | Select-Object Status, FriendlyName, InstanceId
```

Cari yang namanya mengandung "USB-Enhanced-SERIAL" atau "CH34x"/"CP210x" — itu
board ESP32-S3-nya (pada kasus ini: **COM5**).

Flash:

```powershell
espflash flash --port COM5 target/xtensa-esp32s3-none-elf/release/coffee-enose-firmware
```

`espflash` akan:
1. Reset chip ke mode bootloader (lewat sinyal DTR/RTS di kabel USB).
2. Membaca info chip (tipe chip, ukuran flash, MAC address).
3. Meng-convert ELF jadi image biner dan menuliskannya ke flash ESP32-S3 sesuai
   partition table (`firmware/partitions.csv`).
4. Reset chip lagi supaya firmware baru langsung jalan.

## 7. Memantau Log Serial (Serial Monitor)

Setelah firmware jalan, pantau output log-nya:

```powershell
espflash monitor --port COM5
```

Log yang seharusnya muncul urut:
```
Start WiFi connection task
WiFi connected!
Menunggu IP dari DHCP...
Dapat IP: <alamat IP dari router>
Menghubungkan ke ThingsBoard thingsboard.cloud:1883...
MQTT connected ke ThingsBoard
```
lalu tidak ada log lagi kecuali error (karena publish telemetry tidak di-log,
supaya log tidak penuh) — tapi data tetap terkirim tiap 5 detik.

Tekan `Ctrl+C` untuk keluar dari monitor (**bukan** menutup power board).

> **Catatan penting yang kami temukan saat validasi:** secara default, crate
> `esp-println` (dipakai untuk mencetak log) memilih output lewat peripheral
> **USB-Serial-JTAG** bawaan chip ESP32-S3, BUKAN lewat UART yang dijembatani
> chip CH343 (yang dipakai untuk flashing di COM5). Akibatnya log tidak
> pernah muncul di `espflash monitor --port COM5` walau firmware sebenarnya
> jalan normal (terbukti dari data yang tetap masuk ke ThingsBoard). Untuk
> memperbaikinya, di `firmware/Cargo.toml`, dependency `esp-println` di-set
> eksplisit pakai fitur `"uart"` (bukan default/auto):
> ```toml
> esp-println = { version = "=0.14.0", default-features = false, features = [
>     "esp32s3", "uart", "log-04", "colors", "critical-section",
> ] }
> ```

## 8. Verifikasi Data Masuk ke ThingsBoard

1. Buka [thingsboard.cloud](https://thingsboard.cloud) → **Entities → Devices** →
   buka device `coffee-enose`.
2. Tab **Latest telemetry** → harus muncul `gas_ch0`, `gas_ch1`, `gas_ch2`,
   `gas_ch3` dengan **Last update time** yang terus bertambah tiap ±5 detik.
3. Kalau ada dashboard, buka dashboardnya → status **Connectivity** harusnya
   "Online" (bukan "Offline") selama firmware masih menyala dan WiFi tersambung.

## 9. Alur Debugging yang Dilakukan (untuk penjelasan proses ke dosen)

Ringkasan masalah yang muncul saat pertama kali mencoba, dan cara mengatasinya
— ini bagus untuk ditunjukkan sebagai bukti proses debugging yang sistematis:

| # | Masalah | Penyebab | Solusi |
|---|---------|----------|--------|
| 1 | `error: failed to run custom build command for xtensa-lx-rt` — "Either the esp32, esp32s2, esp32s3 feature must be enabled" | Dua versi berbeda dari crate `esp-hal` ter-resolve sekaligus (konflik versi) karena `esp-hal-ota`/`esp-storage` versi terbaru minta generasi `esp-hal` yang lebih baru dari yang didukung `esp-wifi` | Sementara melepas fitur OTA (`esp-hal-ota`, `esp-storage`) dari `Cargo.toml`, fokus ke WiFi+MQTT dulu |
| 2 | `error[E0463]: can't find crate for 'alloc'` | `build-std` di `.cargo/config.toml` cuma include `"core"`, padahal beberapa crate butuh `alloc` (heap allocator) | Tambah `"alloc"` ke `build-std` |
| 3 | `error[E0432]: unresolved import 'ads1x1x::SlaveAddr'` | Nama tipe berubah antar versi crate `ads1x1x` — di versi 0.3.0 namanya `TargetAddr`, bukan `SlaveAddr` | Ganti semua pemakaian `SlaveAddr` jadi `TargetAddr` |
| 4 | Linker error puluhan baris "undefined reference to `strcpy`/`g_osi_funcs_p`/dst" dari `libnet80211.a`/`libphy.a`/`libpp.a` (blob C milik driver WiFi) | `.cargo/config.toml` belum punya `rustflags` linker script yang benar (`-Tlinkall.x`, `-nostartfiles`) yang dibutuhkan `esp-wifi` untuk menyatukan seluruh symbol driver WiFi | Tambah `rustflags = ["-C", "link-arg=-Tlinkall.x", "-C", "link-arg=-nostartfiles"]` |
| 5 | Firmware sukses flash & terbukti kirim data ke ThingsBoard, tapi `espflash monitor` tidak menampilkan log apapun | `esp-println` default pakai output USB-Serial-JTAG, bukan UART yang dipantau lewat COM5 | Set fitur `esp-println` eksplisit ke `"uart"` |
| 6 | Device sempat "Online" ±5 menit lalu jadi "Offline" di ThingsBoard | Diduga MQTT keep-alive tidak terjaga (crate `rust-mqtt` tidak otomatis kirim PINGREQ periodik) — **masih dalam investigasi**, lihat bagian Batasan & PR di bawah | — |

## 10. Batasan yang Masih Ada (jujurkan ke dosen kalau ditanya)

- **Fitur OTA (update firmware over-the-air lewat ThingsBoard) untuk sementara
  dimatikan.** Kode deteksinya ada (baca shared attributes `fw_title`/`fw_version`
  dari ThingsBoard), tapi bagian menulis firmware baru ke flash belum aktif —
  akan diaktifkan lagi setelah crate `esp-hal-ota` dan `esp-wifi` berada di
  generasi versi `esp-hal` yang sama (saat ini masih bentrok, lihat tabel #1 di atas).
- **Koneksi MQTT bisa terputus setelah beberapa menit** kalau tidak ada mekanisme
  keep-alive/ping yang berjalan — ini sedang diselidiki dan diperbaiki.
- Ekosistem Rust untuk ESP32 (`esp-hal`) masih berstatus **beta/rc** (belum versi
  1.0 stabil), jadi kombinasi versi antar crate bisa berubah dan butuh
  penyesuaian dari waktu ke waktu.

## 11. Perintah Ringkas (cheat sheet)

Setelah toolchain ter-install (langkah 3 hanya sekali), untuk build+flash+monitor
ulang cukup:

```powershell
. "C:\Users\<user>\export-esp.ps1"
$env:SSID = "nama_wifi"
$env:PASSWORD = "password_wifi"
$env:TB_HOST = "thingsboard.cloud"
$env:TB_TOKEN = "access_token_device"

cd "PROJECT_COFFEE_ENOSE\firmware"
cargo build --release
espflash flash --port COM5 target/xtensa-esp32s3-none-elf/release/coffee-enose-firmware
espflash monitor --port COM5
```
