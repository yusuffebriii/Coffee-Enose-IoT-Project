# Firmware — Coffee E-Nose (ESP32-S3, akuisisi sensor)

Firmware Rust (`no_std`, esp-hal + embassy) untuk ESP32-S3 yang membaca array
8 sensor gas via **2x ADS1115** (I2C) + 1x **DHT22** (temperatur/kelembapan),
lalu mencetak tiap sampel sebagai baris CSV ke serial, 1 Hz.

> **Status (2026-09-23): rewrite bersih, project akuisisi sensor murni.**
> Firmware versi sebelumnya (WiFi + MQTT ke ThingsBoard Cloud + OTA update)
> **terbukti jalan di hardware asli**, tapi sengaja dilepas dulu dari
> firmware ini supaya fokus ke akuisisi data sesuai protokol kelas. Kode itu
> tidak hilang — masih lengkap di riwayat git sebelum commit rewrite ini
> (`git log -- firmware/src/main.rs`), tinggal digabung ulang begitu
> akuisisi data + verifikasi hardware selesai (lihat Bagian 25-27 dokumen
> instruksi CBP untuk TinyML/IoT/dashboard yang akan menyusul).
>
> **Update (2026-09-23): sudah di-flash ke ESP32-S3 asli dan terbukti
> boot + jalan** (log serial bersih, tidak crash, `Ticker` 1Hz stabil) —
> tapi **baru ESP32-S3-nya sendiri yang tersambung, belum ada sensor**
> (2x ADS1115 + DHT22) yang dikabel. Scan I2C saat boot melaporkan 0
> device, dan semua kanal gas + DHT22 fallback ke `0`/`NaN` — itu memang
> perilaku yang benar untuk board tanpa sensor, bukan bug. Hardware
> sensor menyusul dipasang besok. Jangan dipakai ambil data sungguhan
> sebelum checklist Lampiran C dokumen instruksi CBP (verifikasi tegangan
> + scan I2C) selesai setelah sensor terpasang.

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
DATA,t_s,mq3_raw,mq6_raw,mq7_raw,mq135_raw,tgs2600_raw,tgs2602_raw,tgs2611_raw,tgs2620_raw,mq3_v,mq6_v,mq7_v,mq135_v,tgs2600_v,tgs2602_v,tgs2611_v,tgs2620_v,temp_c,rh_pct,dht_age_s
```

- `t_s`: penghitung sampel (0, 1, 2, ...) sejak `sensor_task` mulai jalan —
  bukan wall-clock, dipakai sebagai waktu relatif dalam satu sesi perekaman
  (dokumen instruksi §17.4).
- `*_raw`: kode ADC mentah (16-bit signed) dari 2x ADS1115. Kalau ADC gagal
  dibaca (wiring/alamat I2C salah), nilainya `0` (fallback, bukan error
  fatal — dicatat lewat `warn!` ke log).
- `*_v`: `*_raw` dikonversi ke volt memakai LSB pada `FullScaleRange::
  Within4_096V` yang di-set saat init (4,096 V / 2^15 per bit). Dokumen
  instruksi §17.5 poin 1 minta raw ADC **dan** tegangan hasil konversi
  sama-sama disimpan, bukan salah satu saja.
- `temp_c`, `rh_pct`: pembacaan DHT22 terakhir yang berhasil (di-hold antar
  pembacaan karena DHT22 maksimum 0,5 Hz — lihat dokumen instruksi §9.2).
  `NaN` kalau belum pernah berhasil membaca sama sekali.
- `dht_age_s`: usia (detik) pembacaan DHT22 yang sedang di-hold di atas —
  dipakai untuk mengaudit penyelarasan waktu (dokumen instruksi §17.4).

[`tools/record_sample.ps1`](tools/record_sample.ps1) membaca baris ini dan
menyimpannya ke CSV di `01_raw_data/`, menambahkan kolom `timestamp` (ISO
8601, dicatat host saat baris diterima).

## Yang sengaja BELUM ada di firmware ini

- **WiFi / MQTT / cloud (ThingsBoard)** — ada di git history sebelum rewrite
  ini, tinggal digabung ulang begitu akuisisi data selesai.
- **OTA firmware update** — sama, ada di git history.
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
  detik), dan pastikan pin data memang GPIO4 (atau sudah diganti sesuai
  wiring Anda di [`src/main.rs`](src/main.rs)).
