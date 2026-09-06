# Coffee E-Nose — Klasifikasi Jenis Kopi via Electronic Nose

**Kelompok:** G-07

## Deskripsi

Project ini mengembangkan sistem *electronic nose* (E-Nose) berbasis array sensor gas untuk
mengklasifikasikan jenis/varietas biji kopi berdasarkan profil aromanya. Sistem terdiri dari
firmware akuisisi data (ESP32-S3 + ADC eksternal ADS1115 via I2C) dan pipeline pengolahan data
di sisi komputer untuk ekstraksi fitur serta pelatihan model klasifikasi.

## Struktur Folder

```
01_raw_data/       Data mentah hasil akuisisi sensor (belum diolah)
02_metadata/        Metadata sampel, label, kondisi pengujian
03_processed_data/  Data hasil pembersihan/preprocessing
04_features/        Fitur hasil ekstraksi dari data sensor
05_models/           Model machine learning yang telah dilatih
06_results/          Hasil evaluasi, metrik, figur
07_notebooks/        Notebook eksplorasi dan analisis
08_reports/          Laporan dan tulisan akhir
firmware/            Firmware Rust untuk ESP32-S3 (akuisisi data via ADS1115/I2C)
docs/                Dokumentasi tambahan
```

## Firmware

Firmware akuisisi data ada di [`firmware/`](firmware/), ditulis dalam Rust untuk target
ESP32-S3, membaca sensor gas melalui ADC eksternal ADS1115 lewat antarmuka I2C. Lihat
[`firmware/README.md`](firmware/README.md) untuk detail build dan flashing.

## Status

Project dalam tahap pengembangan awal (scaffolding struktur folder, README, dan skeleton firmware).
