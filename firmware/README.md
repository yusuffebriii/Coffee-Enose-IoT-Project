# Firmware — Coffee E-Nose (ESP32-S3, sensor + WiFi/MQTT)

Firmware Rust (`no_std`, esp-hal + embassy) untuk ESP32-S3 yang membaca array
8 sensor gas via **2x ADS1115** (I2C) + 1x **DHT22** (temperatur/kelembapan),
mencetak tiap sampel sebagai baris CSV ke serial (1 Hz), dan mengirim
telemetry ke **ThingsBoard Cloud via WiFi/MQTT** (publish tiap 5 detik kalau
connect).

> **Riwayat singkat:** firmware awal (sebelum 2026-09-23) sudah WiFi+MQTT+OTA
> dan **terbukti jalan di hardware asli**. Lalu di-rewrite bersih jadi
> akuisisi-sensor-murni dulu (fokus protokol data kelas), dan hari yang sama
> **di-flash ulang ke ESP32-S3 asli dan terbukti boot + jalan bersih**
> (log serial rapi, tidak crash, `Ticker` 1Hz stabil) — waktu itu baru
> ESP32-S3-nya sendiri yang tersambung, belum ada sensor fisik. Sekarang
> WiFi+MQTT **digabung ulang** dari riwayat git (`git log -- firmware/src/main.rs`),
> **TANPA OTA** dan **TANPA wizard konfigurasi via serial** (lihat catatan di
> kepala [`src/main.rs`](src/main.rs) untuk alasannya — intinya: fitur itu
> perlu partition table custom yang belum divalidasi ulang di iterasi ini).
>
> **Update: WiFi+MQTT sudah dicoba ulang di hardware dan BERHASIL** —
> log serial menunjukkan rantai penuh `WiFi connected!` → dapat IP DHCP →
> `MQTT connected ke ThingsBoard`, telemetry publish tiap 5 detik. Bagian
> sensor (ADS1115/DHT22/scan I2C) statusnya masih sama seperti sebelumnya:
> sudah boot bersih di board tanpa sensor, belum dicoba dengan sensor
> fisik tersambung (menyusul dipasang).

## Protokol pengambilan data

10x percobaan, masing-masing **100 gram** kopi, direkam **300 detik**
(keputusan lisan dosen — beda dari angka §14.1 di dokumen instruksi CBP
resmi yang minta 10 g aliquot/500 detik, lihat catatan di kepala
[`src/main.rs`](src/main.rs)). Firmware tidak tahu soal massa/jumlah
percobaan — itu prosedur manual di luar kode, dan parameter
`-DurationSeconds` di [`tools/record_sample.ps1`](tools/record_sample.ps1).
Firmware sendiri cuma mencetak sampel 1 Hz terus-menerus; host yang
menentukan jendela 300 detik per percobaan.

## Wiring

Pemetaan sensor → ADS1115/DHT22 mengikuti acuan repo referensi
[`TA-Rafi-Cepmek`](https://github.com/renorzz/TA-Rafi-Cepmek) (dibaca
lengkap dari README-nya, bukan cuma bagian ADS1115) — **belum
diverifikasi ke hardware kelompok ini sendiri**, lihat peringatan di
[`src/main.rs`](src/main.rs).

| ESP32-S3 | ADS1115 #1 (`0x48`, ADDR→GND) | ADS1115 #2 (`0x49`, ADDR→VDD) |
|----------|-------------------------------|-------------------------------|
| GPIO8    | SDA                            | SDA (bus sama)                |
| GPIO9    | SCL                             | SCL (bus sama)                |
| 3V3      | VDD                             | VDD                            |
| GND      | GND                             | GND                            |

| Channel ADS1115 #1 (`0x48`) | Sensor  | Channel ADS1115 #2 (`0x49`) | Sensor  |
|------------------------------|---------|------------------------------|---------|
| AIN0                          | MQ-6    | AIN0                          | TGS2611 |
| AIN1                          | MQ-135  | AIN1                          | TGS2602 |
| AIN2                          | MQ-3    | AIN2                          | TGS2600 |
| AIN3                          | MQ-7    | AIN3                          | TGS2620 |

DHT22 (1 unit, sesuai dokumen instruksi CBP §9.2): pin data ke **GPIO10**
lewat resistor pull-up (~4.7–10kΩ ke 3V3) atau andalkan pull-up internal
yang sudah diaktifkan firmware. Repo referensi pakai 2x DHT22 (GPIO10 =
"Suhu & Kelembaban Ruang Chamber", GPIO11 = "Suhu & Kelembaban Ambien");
kelompok ini cuma 1 unit dan GPIO10 dipilih karena posisinya paling cocok
dengan deskripsi §9.2 (di antara array sensor gas dan sample chamber) —
**ganti ke GPIO11 kalau DHT22 Anda sebenarnya di posisi ambien/luar
chamber.**

**Sebelum menyambungkan sensor ke ADS1115**, ikuti Lampiran C dokumen
instruksi CBP: baca datasheet ADS1115 untuk batas tegangan input relatif
VDD, ukur Vout tiap sensor pakai multimeter, hitung rasio pembagi tegangan
yang terpasang, dan pastikan tidak melebihi batas aman sebelum menyalakan.

## Setup WiFi & ThingsBoard (wajib sebelum build)

1. Buka [thingsboard.cloud](https://thingsboard.cloud) → daftar/login.
2. **Entities → Devices → +  → Add new device** → beri nama (mis.
   `coffee-enose`) → **Add**.
3. Buka device itu → tab **Details** → **Copy access token**.
4. Copy [`wifi_config.example.txt`](wifi_config.example.txt) jadi
   `wifi_config.txt` (folder `firmware/` yang sama), isi 4 barisnya:
   ```
   SSID=nama_wifi_anda
   PASSWORD=password_wifi_anda
   TB_HOST=thingsboard.cloud
   TB_TOKEN=access_token_dari_langkah_3
   ```
   File ini **sengaja di-`.gitignore`** (tidak ke-commit) supaya password/
   token tidak ikut ter-upload — `build.rs` otomatis membacanya saat compile
   dan akan **gagal build (panic)** kalau file ini belum ada/lengkap.
5. Ganti WiFi/ThingsBoard nanti = edit file ini lagi + `cargo build --release`
   + reflash. Tidak ada cara ganti tanpa reflash di versi ini (lihat catatan
   di kepala [`src/main.rs`](src/main.rs)).
6. Cek data masuk: device → tab **Latest telemetry**, harus muncul field
   seperti `mq3`, `temp_c`, dst. ter-update tiap ±5 detik setelah board
   connect WiFi.

ESP32-S3 cuma dukung **WiFi 2,4GHz** (bukan 5GHz), dan butuh keamanan
WPA2-AES (WPA/WPA2-TKIP versi lama kadang tidak didukung).

## Prasyarat build (sekali saja per laptop)

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

## Build & flash

```powershell
$env:PATH = [System.Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [System.Environment]::GetEnvironmentVariable('Path','User')
. "$HOME\export-esp.ps1"
cd firmware
cargo build --release
espflash flash --port COM5 --monitor target/xtensa-esp32s3-none-elf/release/coffee-enose-firmware
```

Ganti `COM5` sesuai port board Anda (cek lewat
`Get-PnpDevice -Class Ports -PresentOnly`). Tidak perlu `--partition-table`
lagi — firmware ini tidak pakai OTA, jadi partition table default (single
`factory`) yang dipakai espflash sudah cukup.

Build akan **gagal** kalau `wifi_config.txt` belum ada — lihat bagian
"Setup WiFi & ThingsBoard" di atas.

## Scan I2C saat boot

Tiap kali firmware boot, sebelum mulai streaming data, otomatis scan alamat
I2C `0x03`-`0x77` dan mencetak alamat mana saja yang merespons (ACK) — ini
implementasi item #7 Lampiran C dokumen instruksi ("Pemindaian bus I2C
dilakukan; alamat kedua ADS1115 terkonfirmasi"). Kalau wiring benar, akan
muncul dua baris menyebut `0x48` dan `0x49`; kalau tidak ada satu pun
device yang merespons, langsung ketahuan dari log tanpa harus menebak dari
data `0` di baris `DATA,...` setelahnya.

## Output serial

Satu baris CSV per detik, jadwal absolut lewat `embassy_time::Ticker` (bukan
`Timer::after` berantai, supaya waktu proses baca ADC/DHT22 tidak menumpuk
jadi drift antar sampel — dokumen instruksi §10.4):

```
DATA,t_s,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620,temp_c,rh_pct,dht_age_s
```

- `t_s`: penghitung sampel (0, 1, 2, ...) sejak `sensor_task` mulai jalan —
  bukan wall-clock, dipakai sebagai waktu relatif dalam satu sesi perekaman
  (dokumen instruksi §17.4).
- `mq3`..`tgs2620`: kanal gas dalam **volt** (hasil konversi dari raw ADC
  code memakai LSB pada `FullScaleRange::Within4_096V`, 4,096 V / 2^15 per
  bit) — bukan raw ADC code lagi (keputusan eksplisit 2026-09-23, demi
  kesederhanaan satu nilai per kanal). Kalau ADC gagal dibaca (wiring/
  alamat I2C salah), nilainya `0.0000` (fallback, bukan error fatal —
  dicatat lewat `warn!` ke log).
  > ⚠️ **Catatan kepatuhan:** dokumen instruksi CBP §17.5 poin 1 sebenarnya
  > minta raw ADC code **dan** tegangan hasil konversi disimpan berdua,
  > bukan salah satu saja ("Engineering-unit data ... disimpan sebagai
  > turunan, bukan pengganti data mentah"). Dengan cuma volt yang
  > disimpan, ini **tidak sepenuhnya sesuai §17.5** — raw code bisa
  > dihitung balik (`raw = volt / (4.096/32768)`) selama FullScaleRange
  > tidak berubah, tapi itu bukan hal yang sama dengan menyimpan data
  > mentahnya langsung. Kalau laporan akhir perlu strict sesuai §17.5,
  > beri tahu saya untuk dikembalikan ke dua kolom.
- `temp_c`, `rh_pct`: pembacaan DHT22 terakhir yang berhasil (di-hold antar
  pembacaan karena DHT22 maksimum 0,5 Hz — lihat dokumen instruksi §9.2).
  `NaN` kalau belum pernah berhasil membaca sama sekali.
- `dht_age_s`: usia (detik) pembacaan DHT22 yang sedang di-hold di atas —
  dipakai untuk mengaudit penyelarasan waktu (dokumen instruksi §17.4).

[`tools/record_sample.ps1`](tools/record_sample.ps1) membaca baris ini dan
menyimpannya ke CSV di `01_raw_data/`, menambahkan kolom `timestamp` (ISO
8601, dicatat host saat baris diterima).

## Output MQTT (ThingsBoard)

Field sama seperti CSV (minus `t_s`), dikirim sebagai JSON ke topic
`v1/devices/me/telemetry` tiap 5 detik kalau WiFi+MQTT connect:

```json
{"mq3":1.5430,"mq6":1.5000,"mq7":1.4750,"mq135":1.5625,
 "tgs2600":1.1250,"tgs2602":1.0875,"tgs2611":1.1500,"tgs2620":1.1188,
 "temp_c":29.4,"rh_pct":62.1,"dht_age_s":1}
```

Publish pakai QoS0 (tidak dijamin sampai, tapi ringan) tiap 5 detik dari
bacaan sensor **terbaru** (independen dari sensor_task — kalau WiFi/MQTT
putus, sensor_task tetap jalan dan logging lokal ke serial tidak terganggu).
Task MQTT reconnect otomatis (jeda 3 detik) kalau koneksi putus.

## Yang sengaja BELUM ada di firmware ini

- **OTA firmware update** — ada di git history sebelum WiFi/MQTT digabung
  ulang (`git log -- firmware/src/main.rs`), butuh esp-hal-ota + partition
  table OTA custom untuk diaktifkan lagi.
- **Wizard konfigurasi WiFi via serial** (ganti WiFi tanpa reflash) — juga
  ada di git history, tapi sengaja tidak dipakai lagi (riwayatnya pernah
  buggy, dan menulis flash mentah tanpa partition table custom berisiko
  menimpa partisi app). Config sekarang cuma dari `wifi_config.txt`.
- **TinyML inference on-device** — bagian dokumen instruksi §25-26, belum
  mulai.
- **Sesi terjadwal di device** (mis. auto-stop setelah 300 detik) — durasi
  perekaman dikontrol dari host (`record_sample.ps1 -DurationSeconds 300`),
  bukan firmware, supaya firmware tetap sederhana (cuma streaming terus).

## Troubleshooting

- **`cargo`/`espflash` "not recognized"**: PATH belum ke-refresh, buka
  terminal baru, atau jalankan ulang `. "$HOME\export-esp.ps1"`.
- **Semua nilai ADS1115 = 0**: cek wiring I2C (SDA=GPIO8, SCL=GPIO9), scan
  bus I2C untuk konfirmasi alamat `0x48`/`0x49` benar-benar terdeteksi.
- **`Gagal baca DHT22` terus-menerus di log**: cek pull-up pin data DHT22,
  jangan poll lebih cepat dari yang firmware lakukan (sudah dibatasi 1x/2
  detik), dan pastikan pin data memang GPIO10 (atau sudah diganti sesuai
  wiring Anda di [`src/main.rs`](src/main.rs)).
- **Build gagal, pesan soal `wifi_config.txt` tidak ditemukan**: copy
  `wifi_config.example.txt` jadi `wifi_config.txt` dan isi 4 field-nya
  (lihat bagian "Setup WiFi & ThingsBoard").
- **`Gagal connect WiFi ("Disconnected")` padahal SSID/password benar**:
  router pakai keamanan WPA/WPA2-TKIP (versi lama, kadang tidak didukung
  ESP32-S3) — coba jaringan lain (mis. hotspot HP, biasanya WPA2-AES).
  ESP32-S3 juga cuma support **2,4GHz**, bukan 5GHz.
- **MQTT connect gagal / device Offline di ThingsBoard**: cek `TB_TOKEN`
  benar (copy-paste dari device details di ThingsBoard), dan board punya
  akses internet keluar.
