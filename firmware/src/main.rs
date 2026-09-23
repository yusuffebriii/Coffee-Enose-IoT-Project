#![no_std]
#![no_main]

// Firmware: ESP32-S3, baca array 8 sensor gas (2x ADS1115, I2C) + 1x DHT22
// (temperatur/kelembapan), cetak tiap sampel sebagai baris CSV ke serial.
//
// REWRITE BERSIH (2026-09-23) - versi sebelumnya (WiFi + MQTT ke ThingsBoard
// + OTA update, terbukti jalan di hardware asli) SENGAJA dilepas dulu dari
// firmware ini supaya fokus akuisisi data sesuai protokol kelas. Kode lama
// itu TIDAK hilang, masih ada lengkap di riwayat git sebelum commit rewrite
// ini (`git log -- firmware/src/main.rs`) - tinggal digabung ulang begitu
// akuisisi data + verifikasi hardware selesai.
//
// Protokol pengambilan data (keputusan eksplisit, BUKAN angka di dokumen
// instruksi CBP resmi §14.1 yang minta 10 g aliquot/500 detik): 10x
// percobaan, masing-masing 100 gram kopi, direkam 300 detik. Firmware ini
// tidak tahu soal massa/jumlah percobaan (itu prosedur manual + parameter
// `-DurationSeconds` di tools/record_sample.ps1) - firmware cuma mencetak
// sampel 1 Hz terus-menerus, host yang menentukan jendela 300 detik per
// percobaan.
//
// Pemetaan sensor -> ADS1115 & pin I2C mengikuti acuan repo referensi
// github.com/renorzz/TA-Rafi-Cepmek (README §3.2):
//   - I2C bus: SDA=GPIO8, SCL=GPIO9, 400 kHz (fast mode, sesuai README repo
//     referensi - default esp-hal sebenarnya 100 kHz, di-override eksplisit
//     di main()).
//   - ADS1115 #1 alamat 0x48 (ADDR->GND): AIN0=MQ-6, AIN1=MQ-135, AIN2=MQ-3,
//     AIN3=MQ-7.
//   - ADS1115 #2 alamat 0x49 (ADDR->VDD): AIN0=TGS2611, AIN1=TGS2602,
//     AIN2=TGS2600, AIN3=TGS2620.
// Urutan channel ini BEDA dari firmware versi sebelumnya di repo ini (yang
// pakai urutan MQ-3/6/7/135 berurutan di modul pertama) - dua-duanya sama
// valid secara arsitektur (alamat I2C & pin SDA/SCL sama), bedanya cuma
// keputusan wiring fisik. Dokumen instruksi CBP resmi §10.3 EKSPLISIT minta
// urutan channel diverifikasi ulang terhadap wiring fisik kelompok ini
// (multimeter + scan bus I2C) sebelum dipakai ambil data sungguhan - urutan
// dari repo referensi di atas dipakai di sini HANYA karena diminta eksplisit
// sebagai acuan project baru ini, bukan karena sudah diverifikasi ke
// hardware kelompok ini sendiri.
//
// DHT22: 1 unit (sesuai dokumen instruksi CBP §9.2 - sistem kelas ini pasif/
// statis dengan 1 sensor lingkungan, BEDA dari repo referensi yang pakai 2
// unit DHT22 untuk sistem alir aktif mereka). Dibaca lewat 1 pin open-drain,
// interval baca >=2 detik (batas laju sensor, lihat §9.2), nilai di-hold ke
// grid 1 Hz kanal gas - `dht_age_s` pada tiap baris CSV menyatakan usia
// (detik) pembacaan DHT22 terakhir yang sedang di-hold, supaya penyelarasan
// waktu bisa diaudit (lihat dokumen instruksi §17.4).
//
// Status hardware (2026-09-23): sudah di-flash ke ESP32-S3 asli dan
// TERBUKTI BOOT + JALAN (log serial bersih, Ticker 1Hz stabil, tidak
// crash) - tapi BARU ESP32-S3-nya sendiri yang tersambung, belum ada
// sensor (2x ADS1115 + DHT22) yang dikabel ke board. Scan I2C saat boot
// (`i2c_bus_scan`, item #7 Lampiran C) melaporkan 0 device ditemukan pada
// kondisi ini - itu HASIL YANG BENAR untuk board tanpa sensor, bukan bug.
//
// TODO WAJIB sebelum dipakai ambil data sungguhan (belum tervalidasi ke
// sensor fisik - baru boot ESP32-S3 polosan):
//  - Sambungkan 2x ADS1115 + DHT22 ke board, lalu jalankan Lampiran C
//    dokumen instruksi CBP (7 langkah verifikasi tegangan + scan bus I2C -
//    langkah scan-nya otomatis tercetak tiap boot lewat `i2c_bus_scan`)
//    SEBELUM menyambungkan sensor ke ADC dengan tegangan sungguhan.
//  - Sesuaikan pin I2C (SDA/SCL) & pin DHT22 (GPIO4 di bawah cuma TEBAKAN
//    awal, GANTI sesuai wiring board Anda) kalau beda dari asumsi di atas.
//  - Sesuaikan FullScaleRange ADS1115 dengan rentang tegangan output sensor
//    setelah signal conditioning (lihat dokumen instruksi §10.3).
//  - Konfirmasi urutan channel AIN0-AIN3 tiap modul ADS1115 cocok dengan
//    wiring fisik kelompok ini (lihat catatan di atas) - kalau beda, tinggal
//    tukar argumen `channel::SingleAx` di `sensor_task`.
//  - Verifikasi API crate `dht-sensor` (`dht22::blocking::read`) terhadap
//    versi yang benar-benar ter-resolve di Cargo.lock.

use ads1x1x::{channel, Ads1x1x, FullScaleRange, TargetAddr};
use core::cell::RefCell;
use dht_sensor::dht22;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Ticker, Timer};
use embedded_hal_bus::i2c::RefCellDevice;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{DriveMode, Flex, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    time::Rate,
    timer::timg::TimerGroup,
};
use log::warn;
use nb::block;

esp_bootloader_esp_idf::esp_app_desc!();

/// Sensor lingkungan tidak boleh dibaca lebih cepat dari ini (dokumen
/// instruksi CBP §9.2: DHT22 maksimum 0,5 Hz). Nilai lama di-hold di antara
/// pembacaan supaya kanal gas tetap bisa dicetak tiap 1 detik.
const DHT_MIN_INTERVAL: Duration = Duration::from_secs(2);

/// Scan alamat I2C 7-bit 0x03-0x77 (di luar rentang ini reserved), cetak
/// tiap alamat yang ACK. `write(addr, &[])` cukup untuk deteksi kehadiran -
/// device manapun yang benar-benar ada di bus akan ACK alamatnya sendiri
/// walau payload-nya kosong, tanpa perlu tahu protokol internal device itu.
fn i2c_bus_scan(i2c: &mut I2c<'_, esp_hal::Blocking>) {
    esp_println::println!("Scan bus I2C (0x03-0x77)...");
    let mut found = 0u8;
    for addr in 0x03u8..=0x77u8 {
        if i2c.write(addr, &[]).is_ok() {
            let note = match addr {
                0x48 => " <- diharapkan ADS1115 #1 (MQ-6/MQ-135/MQ-3/MQ-7)",
                0x49 => " <- diharapkan ADS1115 #2 (TGS2611/2602/2600/2620)",
                _ => "",
            };
            esp_println::println!("  alamat 0x{addr:02X} merespons (ACK){note}");
            found += 1;
        }
    }
    if found == 0 {
        warn!("Scan I2C: TIDAK ADA device merespons - cek wiring SDA=GPIO8/SCL=GPIO9, catu daya modul, pull-up.");
    } else {
        esp_println::println!("Scan I2C selesai: {found} device ditemukan.");
        if found < 2 {
            warn!("Scan I2C: kurang dari 2 device (diharapkan 2x ADS1115) - cek modul yang belum terdeteksi.");
        }
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let hal_config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(hal_config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    // I2C untuk 2x ADS1115 (array 8 sensor gas). 400 kHz sesuai acuan repo
    // referensi (default esp-hal sebenarnya 100 kHz) - ADS1115 mendukung
    // fast mode 400 kHz. TODO: sesuaikan pin kalau wiring board Anda beda.
    let mut i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(400)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO8)
    .with_scl(peripherals.GPIO9);

    // Lampiran C dokumen instruksi CBP, item #7: "Pemindaian bus I2C
    // dilakukan; alamat kedua ADS1115 terkonfirmasi" - dijalankan sekali
    // tiap boot, sebelum sensor_task mulai baca terus-menerus, supaya bisa
    // langsung ketahuan dari log serial apakah board melihat modul ADS1115
    // di bus sama sekali (tanpa ini, kanal 0 bisa disalahartikan sebagai
    // "sensor terbaca tapi nilainya kosong" padahal sebenarnya device-nya
    // tidak pernah ke-detect).
    i2c_bus_scan(&mut i2c);

    // Pin DHT22, open-drain + pull-up internal (protokol 1-wire DHT22 minta
    // pin yang bisa gantian jadi input/output - Flex + DriveMode::OpenDrain
    // + set_input_enable(true) supaya satu pin fisik mengimplementasikan
    // embedded_hal::digital::{InputPin, OutputPin} sekaligus, seperti yang
    // diminta crate dht-sensor). GPIO4 = TEBAKAN AWAL, ganti sesuai wiring
    // board Anda.
    let mut dht_pin = Output::new(
        peripherals.GPIO4,
        Level::High,
        OutputConfig::default()
            .with_drive_mode(DriveMode::OpenDrain)
            .with_pull(Pull::Up),
    )
    .into_flex();
    dht_pin.set_input_enable(true);

    esp_println::println!(
        "Coffee E-Nose DAQ siap. Mencetak \"DATA,t_s,mq3_raw,mq6_raw,mq7_raw,mq135_raw,tgs2600_raw,tgs2602_raw,tgs2611_raw,tgs2620_raw,mq3_v,mq6_v,mq7_v,mq135_v,tgs2600_v,tgs2602_v,tgs2611_v,tgs2620_v,temp_c,rh_pct,dht_age_s\" tiap 1 detik."
    );

    spawner.spawn(sensor_task(i2c, dht_pin)).ok();

    // main() tidak melakukan apa-apa lagi - seluruh kerja ada di sensor_task.
    // Nanti WiFi/MQTT/OTA yang digabung ulang akan jalan di sini.
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

/// LSB tegangan ADS1115 pada FullScaleRange::Within4_096V (lihat
/// `set_full_scale_range` di bawah): 4,096 V / 2^15 (rentang single-ended
/// cuma pakai setengah rentang differential 16-bit ADS1115).
const ADS1115_VOLTS_PER_LSB: f32 = 4.096 / 32768.0;

fn raw_to_volts(raw: i16) -> f32 {
    raw as f32 * ADS1115_VOLTS_PER_LSB
}

/// Baca 8 sensor gas (2x ADS1115, alamat 0x48 & 0x49) + 1x DHT22 tiap 1
/// detik (jadwal absolut lewat `Ticker`, bukan `Timer::after` berantai -
/// dokumen instruksi CBP §10.4 eksplisit minta "timer monotonik... bukan
/// delay berantai" supaya waktu proses baca ADC/DHT22 tidak menumpuk jadi
/// drift antar sampel), cetak ke serial sebagai baris CSV
/// "DATA,t_s,mq3_raw,...,tgs2620_raw,mq3_v,...,tgs2620_v,temp_c,rh_pct,dht_age_s".
/// Kolom mengikuti skema data mentah dokumen instruksi CBP §17.4 (raw ADC
/// code DAN tegangan hasil konversi, keduanya disimpan - lihat §17.5 poin 1)
/// dikurangi kolom `timestamp` absolut yang ditambahkan host lewat
/// tools/record_sample.ps1, bukan di sini.
#[embassy_executor::task]
async fn sensor_task(i2c: I2c<'static, esp_hal::Blocking>, mut dht_pin: Flex<'static>) {
    let i2c_bus = RefCell::new(i2c);

    // Modul #1 (ADDR->GND, alamat default 0x48): AIN0=MQ-6, AIN1=MQ-135,
    // AIN2=MQ-3, AIN3=MQ-7 (urutan sesuai acuan repo referensi, lihat catatan
    // di kepala file).
    let mut adc_a = Ads1x1x::new_ads1115(RefCellDevice::new(&i2c_bus), TargetAddr::default());
    // Modul #2 (ADDR->VDD, alamat 0x49): AIN0=TGS2611, AIN1=TGS2602,
    // AIN2=TGS2600, AIN3=TGS2620.
    let mut adc_b = Ads1x1x::new_ads1115(RefCellDevice::new(&i2c_bus), TargetAddr::Vdd);

    if adc_a.set_full_scale_range(FullScaleRange::Within4_096V).is_err() {
        warn!("Gagal set full scale range ADS1115 #1 / 0x48 (cek wiring/alamat I2C)");
    }
    if adc_b.set_full_scale_range(FullScaleRange::Within4_096V).is_err() {
        warn!("Gagal set full scale range ADS1115 #2 / 0x49 (cek wiring/alamat I2C)");
    }

    // Delay presisi-mikrodetik untuk bit-banging protokol DHT22 - BUKAN
    // embassy_time::Timer (resolusinya milidetik dan async, terlalu kasar
    // untuk timing DHT22 yang butuh puluhan mikrodetik).
    let mut dht_delay = Delay::new();

    let mut held_temp_c: f32 = f32::NAN;
    let mut held_rh_pct: f32 = f32::NAN;
    // Dipisah dari `last_dht_success` di bawah supaya retry setelah gagal
    // baca TETAP menunggu DHT_MIN_INTERVAL, bukan langsung coba lagi detik
    // berikutnya - dht-sensor sendiri mendokumentasikan bahwa polling
    // terlalu cepat adalah PENYEBAB timeout, bukan cuma gejalanya, jadi
    // retry rapat-rapat bisa bikin loop gagal terus-menerus.
    let mut last_dht_attempt: Option<Instant> = None;
    let mut last_dht_success: Option<Instant> = None;

    // Jadwal absolut 1 Hz: `ticker.next()` menunggu sampai deadline
    // `expires_at`, yang di-increment tetap +1 detik tiap tick TERLEPAS dari
    // berapa lama badan loop di atasnya makan waktu - beda dari
    // `Timer::after(1s)` yang dipanggil ulang tiap iterasi (delay berantai,
    // bisa drift kalau badan loop makan waktu variabel).
    let mut ticker = Ticker::every(Duration::from_secs(1));
    let mut t_s: u64 = 0;

    loop {
        let mq6 = block!(adc_a.read(channel::SingleA0)).unwrap_or(0);
        let mq135 = block!(adc_a.read(channel::SingleA1)).unwrap_or(0);
        let mq3 = block!(adc_a.read(channel::SingleA2)).unwrap_or(0);
        let mq7 = block!(adc_a.read(channel::SingleA3)).unwrap_or(0);
        let tgs2611 = block!(adc_b.read(channel::SingleA0)).unwrap_or(0);
        let tgs2602 = block!(adc_b.read(channel::SingleA1)).unwrap_or(0);
        let tgs2600 = block!(adc_b.read(channel::SingleA2)).unwrap_or(0);
        let tgs2620 = block!(adc_b.read(channel::SingleA3)).unwrap_or(0);

        let should_poll_dht = match last_dht_attempt {
            None => true,
            Some(t) => t.elapsed() >= DHT_MIN_INTERVAL,
        };
        if should_poll_dht {
            last_dht_attempt = Some(Instant::now());
            match dht22::blocking::read(&mut dht_delay, &mut dht_pin) {
                Ok(reading) => {
                    held_temp_c = reading.temperature;
                    held_rh_pct = reading.relative_humidity;
                    last_dht_success = Some(Instant::now());
                }
                Err(_) => warn!("Gagal baca DHT22 (cek wiring/pull-up pin)"),
            }
        }
        let dht_age_s = last_dht_success
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(u64::MAX);

        let mq3_v = raw_to_volts(mq3);
        let mq6_v = raw_to_volts(mq6);
        let mq7_v = raw_to_volts(mq7);
        let mq135_v = raw_to_volts(mq135);
        let tgs2600_v = raw_to_volts(tgs2600);
        let tgs2602_v = raw_to_volts(tgs2602);
        let tgs2611_v = raw_to_volts(tgs2611);
        let tgs2620_v = raw_to_volts(tgs2620);

        esp_println::println!(
            "DATA,{t_s},{mq3},{mq6},{mq7},{mq135},{tgs2600},{tgs2602},{tgs2611},{tgs2620},\
             {mq3_v:.4},{mq6_v:.4},{mq7_v:.4},{mq135_v:.4},{tgs2600_v:.4},{tgs2602_v:.4},{tgs2611_v:.4},{tgs2620_v:.4},\
             {held_temp_c},{held_rh_pct},{dht_age_s}"
        );

        t_s += 1;
        ticker.next().await;
    }
}
