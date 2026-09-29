# Coffee E-Nose — Klasifikasi Jenis Kopi via Electronic Nose

**Kelompok:** G-07

## Deskripsi

Project ini mengembangkan sistem *electronic nose* (E-Nose) berbasis array sensor gas untuk
mengklasifikasikan jenis/varietas biji kopi berdasarkan profil aromanya. Sistem terdiri dari
firmware akuisisi data (ESP32-S3 + ADC eksternal ADS1115 via I2C) dan pipeline pengolahan data
di sisi komputer untuk ekstraksi fitur serta pelatihan model klasifikasi.

## Struktur Folder
01_raw_data/ Data mentah hasil akuisisi sensor (belum diolah)
02_metadata/ Metadata sampel, label, kondisi pengujian
03_processed_data/ Data hasil pembersihan/preprocessing
04_features/ Fitur hasil ekstraksi dari data sensor
05_models/ Model machine learning yang telah dilatih
06_results/ Hasil evaluasi, metrik, figur
07_notebooks/ Notebook eksplorasi dan analisis
08_reports/ Laporan dan tulisan akhir
firmware/ Firmware Rust untuk ESP32-S3 (akuisisi data via ADS1115/I2C)
docs/ Dokumentasi tambahan


## Firmware

Firmware ada di [`firmware/`](firmware/), ditulis dalam Rust untuk target ESP32-S3. Selain
akuisisi data (8 kanal sensor gas via 2x ADC eksternal ADS1115 lewat I2C, plus sensor
lingkungan DHT22), firmware juga menangani konektivitas WiFi/MQTT ke ThingsBoard Cloud dan
mendukung **OTA (Over-The-Air) firmware update** lewat ThingsBoard OTA package. Lihat
[`firmware/README.md`](firmware/README.md) untuk detail build, flashing, dan cara kerja OTA.

## Status

- **Firmware akuisisi sensor**: selesai — membaca 8 kanal sensor gas (MQ-3, MQ-6, MQ-7,
  MQ-135, TGS2600, TGS2602, TGS2611, TGS2620) via 2x ADS1115, plus DHT22, dicetak ke serial
  dalam format CSV.
- **Konektivitas WiFi/MQTT ke ThingsBoard**: selesai dan teruji — telemetri terkirim
  berkala, auto-reconnect kalau koneksi putus.
- **OTA firmware update**: selesai dan teruji end-to-end di perangkat keras asli — device
  berhasil mendeteksi firmware baru yang di-assign di ThingsBoard, mengunduh per-chunk,
  memverifikasi checksum CRC32, lalu reboot ke firmware baru tanpa kehilangan konektivitas.
- **Pipeline pengolahan data & model klasifikasi** (`03_processed_data/` s.d. `06_results/`):
  belum dimulai — menunggu data sampel kopi sesungguhnya, yang membutuhkan pemasangan fisik
  seluruh sensor ke board terlebih dahulu.
