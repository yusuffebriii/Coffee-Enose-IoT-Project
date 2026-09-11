# Panduan Ganti WiFi ESP32 (Coffee E-Nose)

Board ESP32-S3 ini lagi di-setting pakai WiFi yang tidak ada di lokasimu.
Tolong ganti ke WiFi tempatmu sekarang, caranya:

## Yang dibutuhkan
- Laptop dengan aplikasi terminal serial — kalau belum ada, install salah satu:
  - **PuTTY** (gratis): https://www.putty.org/ — download & install versi Windows
  - Atau kalau sudah pernah install Arduino IDE, bisa pakai **Serial Monitor** bawaannya

## Langkah-langkah

1. Pastikan ESP32-S3 tersambung USB ke laptopmu.
2. Buka **Device Manager** (search di Start Menu) → klik **Ports (COM & LPT)** →
   catat nomor COM yang muncul (misal "COM5" atau "COM7", biasanya namanya ada
   "USB-Enhanced-SERIAL" atau "USB Serial").
3. Buka **PuTTY**:
   - Connection type: pilih **Serial**
   - Serial line: isi nomor COM tadi (misal `COM5`)
   - Speed: isi **115200**
   - Klik **Open**
4. Sebuah jendela hitam (terminal) akan terbuka. **Cabut lalu colok ulang** kabel
   USB ESP32 (supaya reboot).
5. Dalam **3 detik** setelah itu, akan muncul tulisan:
   `Tekan 'c' lalu Enter dalam 3 detik untuk ubah WiFi/ThingsBoard...`
   → **ketik `c`**, lalu tekan **Enter**.
6. Nanti muncul 4 pertanyaan berurutan, isi lalu Enter tiap kali:
   - **Nama WiFi (SSID)**: isi nama WiFi tempatmu
   - **Password WiFi**: isi passwordnya
   - **Host ThingsBoard**: ketik `thingsboard.cloud`
   - **Access token ThingsBoard**: ketik `MK5Mu4N2oxFV2NsDnA7u`
7. Setelah isian terakhir, akan muncul "Config tersimpan ke flash. Reboot..."
   — board otomatis restart dan connect pakai WiFi barumu.

## Catatan

- ESP32-S3 **cuma support WiFi 2.4GHz**, bukan 5GHz. Kalau WiFi kamu ada
  pilihan "...5G" dan yang biasa, pakai yang **biasa** (tanpa 5G).
- Kalau ada peringatan "keamanan lemah/WPA-TKIP" pas connect WiFi itu di HP,
  ESP32 mungkin juga gagal connect — coba WiFi lain kalau begitu (misal
  hotspot HP kamu sendiri).
- Setelah selesai, kabari yang minta tolong ini — biar dicek datanya sudah
  masuk ke ThingsBoard atau belum.
