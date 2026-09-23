#![no_std]
#![no_main]

// Firmware: ESP32-S3, baca array 8 sensor gas (2x ADS1115, I2C) + 1x DHT22
// (temperatur/kelembapan), cetak tiap sampel sebagai baris CSV ke serial,
// DAN kirim telemetry ke ThingsBoard Cloud via WiFi/MQTT.
//
// REWRITE BERSIH (2026-09-23) lalu WiFi+MQTT DIGABUNG ULANG (2026-09-23,
// commit setelah scan-I2C) - firmware versi sebelum rewrite (WiFi + MQTT +
// OTA update, terbukti jalan di hardware asli) sempat dilepas total supaya
// fokus akuisisi sensor dulu. Sekarang WiFi+MQTT digabung ulang dari
// riwayat git itu (`git log -- firmware/src/main.rs`), TAPI TANPA OTA dan
// TANPA wizard konfigurasi via serial:
//  - OTA tidak diminta lagi saat ini - kalau perlu nanti, ambil lagi dari
//    commit sebelum rewrite (butuh esp-hal-ota + partition table OTA).
//  - Wizard serial (ganti WiFi tanpa reflash, simpan ke flash mentah di
//    offset tetap) SENGAJA tidak dipakai lagi - riwayatnya pernah buggy
//    (lihat commit "Dokumentasikan bug 'config flash selalu menang'"), dan
//    menulis flash mentah di luar partition table custom berisiko menimpa
//    partisi app yang sedang jalan kalau ukuran partisi default berubah.
//    Config WiFi/ThingsBoard sekarang HANYA dari wifi_config.txt (dibaca
//    saat compile lewat build.rs, lihat src/config.rs) - ganti WiFi berarti
//    edit file itu + reflash.
//
// Protokol pengambilan data (keputusan eksplisit, BUKAN angka di dokumen
// instruksi CBP resmi §14.1 yang minta 10 g aliquot/500 detik): 10x
// percobaan, masing-masing 100 gram kopi, direkam 300 detik. Firmware ini
// tidak tahu soal massa/jumlah percobaan (itu prosedur manual + parameter
// `-DurationSeconds` di tools/record_sample.ps1) - firmware cuma mencetak
// sampel 1 Hz terus-menerus (dan publish MQTT tiap 5 detik kalau connect),
// host yang menentukan jendela 300 detik per percobaan.
//
// Pemetaan sensor -> ADS1115, pin I2C, dan pin DHT22 mengikuti acuan repo
// referensi github.com/renorzz/TA-Rafi-Cepmek (README, tabel mapping
// sensor & GPIO - dibaca ulang lengkap 2026-09-23, bukan cuma §3.2):
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
// unit DHT22 untuk sistem alir aktif mereka: GPIO10 "Suhu & Kelembaban
// Ruang Chamber" dan GPIO11 "Suhu & Kelembaban Ambien"). Kelompok ini pakai
// GPIO10 - posisinya (di ruang chamber, antara array sensor gas & sample
// chamber) PALING COCOK dengan deskripsi §9.2: "DHT22 berada di antara
// array sensor gas dan sample chamber". Kalau wiring fisik kelompok ini
// ternyata DHT22-nya di posisi ambien/luar chamber, ganti ke GPIO11.
// Dibaca lewat 1 pin open-drain, interval baca >=2 detik (batas laju
// sensor, lihat §9.2), nilai di-hold ke grid 1 Hz kanal gas - `dht_age_s`
// pada tiap baris CSV menyatakan usia (detik) pembacaan DHT22 terakhir
// yang sedang di-hold, supaya penyelarasan waktu bisa diaudit (lihat
// dokumen instruksi §17.4).
//
// Status hardware (2026-09-23): sudah di-flash ke ESP32-S3 asli dan
// TERBUKTI BOOT + JALAN (log serial bersih, Ticker 1Hz stabil, tidak
// crash) - tapi BARU ESP32-S3-nya sendiri yang tersambung, belum ada
// sensor (2x ADS1115 + DHT22) yang dikabel ke board. Scan I2C saat boot
// (`i2c_bus_scan`, item #7 Lampiran C) melaporkan 0 device ditemukan pada
// kondisi ini - itu HASIL YANG BENAR untuk board tanpa sensor, bukan bug.
// Bagian WiFi/MQTT di bawah BELUM PERNAH dicoba di hardware sejak digabung
// ulang (beda dari versi sebelum rewrite yang sudah terbukti jalan - logic-
// nya sama persis, tapi belum di-flash ulang setelah digabung).
//
// TODO WAJIB sebelum dipakai ambil data sungguhan (belum tervalidasi ke
// sensor fisik - baru boot ESP32-S3 polosan):
//  - Sambungkan 2x ADS1115 + DHT22 ke board, lalu jalankan Lampiran C
//    dokumen instruksi CBP (7 langkah verifikasi tegangan + scan bus I2C -
//    langkah scan-nya otomatis tercetak tiap boot lewat `i2c_bus_scan`)
//    SEBELUM menyambungkan sensor ke ADC dengan tegangan sungguhan.
//  - Sesuaikan pin I2C (SDA/SCL) & pin DHT22 (GPIO10 di bawah dari data
//    repo referensi, GANTI ke GPIO11 kalau DHT22 Anda di posisi ambien,
//    bukan ruang chamber - lihat catatan DHT22 di atas) kalau beda dari
//    wiring board Anda.
//  - Sesuaikan FullScaleRange ADS1115 dengan rentang tegangan output sensor
//    setelah signal conditioning (lihat dokumen instruksi §10.3).
//  - Konfirmasi urutan channel AIN0-AIN3 tiap modul ADS1115 cocok dengan
//    wiring fisik kelompok ini (lihat catatan di atas) - kalau beda, tinggal
//    tukar argumen `channel::SingleAx` di `sensor_task`.
//  - Verifikasi API crate `dht-sensor` (`dht22::blocking::read`) terhadap
//    versi yang benar-benar ter-resolve di Cargo.lock.
//  - Copy wifi_config.example.txt -> wifi_config.txt, isi SSID/PASSWORD/
//    TB_HOST/TB_TOKEN (lihat firmware/README.md) sebelum build - build.rs
//    akan gagal (panic saat compile) kalau file itu belum ada.
//  - Verifikasi method rust-mqtt (`connect_to_broker`, `send_message`,
//    `receive_message`, `send_ping`) terhadap versi crate yang benar-benar
//    ter-resolve - sudah pernah jalan di versi sebelum rewrite, tapi belum
//    dicoba lagi setelah digabung ke struktur sensor_task yang baru.

mod config;

use core::cell::RefCell;
use core::fmt::Write as _;

use ads1x1x::{channel, Ads1x1x, FullScaleRange, TargetAddr};
use config::DeviceConfig;
use dht_sensor::dht22;
use embassy_executor::Spawner;
use embassy_futures::select::{select3, Either3};
use embassy_net::{
    dns::DnsQueryType,
    tcp::TcpSocket,
    Config as EmbassyNetConfig, Runner, StackResources,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Instant, Ticker, Timer};
use embedded_hal_bus::i2c::RefCellDevice;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{DriveMode, Flex, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    rng::Rng,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_wifi::{
    init,
    wifi::{ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiState},
    EspWifiController,
};
use heapless::String as HString;
use log::{error, info, warn};
use nb::block;
use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::{publish_packet::QualityOfService, reason_codes::ReasonCode},
    utils::rng_generator::CountingRng,
};

esp_bootloader_esp_idf::esp_app_desc!();

const TB_PORT: u16 = 1883;

/// Sensor lingkungan tidak boleh dibaca lebih cepat dari ini (dokumen
/// instruksi CBP §9.2: DHT22 maksimum 0,5 Hz). Nilai lama di-hold di antara
/// pembacaan supaya kanal gas tetap bisa dicetak tiap 1 detik.
const DHT_MIN_INTERVAL: Duration = Duration::from_secs(2);

/// Satu sampel lengkap (8 kanal gas dalam volt, DHT22, umur DHT22) - dibagi
/// antara `sensor_task` (yang membacanya tiap 1 detik, independen dari
/// WiFi/MQTT) dan loop MQTT di `mqtt_task` (yang publish tiap 5 detik kalau
/// connect). Pakai Signal (bukan Channel) karena yang dibutuhkan cuma nilai
/// TERBARU, bukan antrian semua history.
///
/// Cuma 1 nilai per kanal gas (tegangan hasil konversi, BUKAN raw ADC code
/// + volt terpisah seperti sebelumnya) - keputusan eksplisit 2026-09-23
/// untuk kesederhanaan. CATATAN KEPATUHAN: dokumen instruksi CBP §17.5
/// poin 1 sebenarnya minta raw ADC code DAN tegangan hasil konversi
/// DISIMPAN KEDUANYA ("Raw voltage/ADC data ... Engineering-unit data ...
/// disimpan sebagai turunan, bukan pengganti data mentah"). Dengan cuma
/// menyimpan volt, ini TIDAK sepenuhnya sesuai §17.5 - raw code masih bisa
/// dihitung balik dari volt (raw = volt / ADS1115_VOLTS_PER_LSB) selama
/// FullScaleRange tidak berubah, tapi itu bukan hal yang sama dengan
/// menyimpan data mentahnya langsung.
#[derive(Clone, Copy)]
struct GasReading {
    mq3: f32,
    mq6: f32,
    mq7: f32,
    mq135: f32,
    tgs2600: f32,
    tgs2602: f32,
    tgs2611: f32,
    tgs2620: f32,
    temp_c: f32,
    rh_pct: f32,
    dht_age_s: u64,
}

static READING: Signal<CriticalSectionRawMutex, GasReading> = Signal::new();

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write($val);
        x
    }};
}

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

    esp_alloc::heap_allocator!(size: 96 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);

    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        init(timg0.timer0, rng.clone(), peripherals.RADIO_CLK).unwrap()
    );

    let (controller, interfaces) = esp_wifi::wifi::new(esp_wifi_ctrl, peripherals.WIFI).unwrap();
    let wifi_interface = interfaces.sta;

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
    // diminta crate dht-sensor). GPIO10 - lihat catatan DHT22 di kepala file.
    let mut dht_pin = Output::new(
        peripherals.GPIO10,
        Level::High,
        OutputConfig::default()
            .with_drive_mode(DriveMode::OpenDrain)
            .with_pull(Pull::Up),
    )
    .into_flex();
    dht_pin.set_input_enable(true);

    // Wajib diinisialisasi SEBELUM pemakaian Timer::after().await pertama -
    // kalau tidak, panic "schedule_wake called before esp_hal_embassy::init()".
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timg1.timer0);

    esp_println::println!(
        "Coffee E-Nose DAQ siap. Mencetak \"DATA,t_s,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620,temp_c,rh_pct,dht_age_s\" tiap 1 detik."
    );

    // Sensor gas jalan independen dari WiFi/MQTT - tetap baca & log ke
    // serial (baris "DATA,...") walau ThingsBoard/WiFi lagi bermasalah. Ini
    // yang dipakai untuk logging lokal per-percobaan kopi (lihat
    // tools/record_sample.ps1), sekaligus jadi sumber data buat dikirim ke
    // ThingsBoard lewat `READING` kalau MQTT connect.
    spawner.spawn(sensor_task(i2c, dht_pin)).ok();

    // Config WiFi/ThingsBoard HANYA dari wifi_config.txt (env var saat
    // compile via build.rs) - lihat catatan di kepala file soal kenapa
    // wizard serial + flash persistence tidak dipakai lagi.
    let device_cfg: &'static DeviceConfig = mk_static!(DeviceConfig, DeviceConfig::from_build_env());

    let net_config = EmbassyNetConfig::dhcpv4(Default::default());
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        mk_static!(StackResources<4>, StackResources::<4>::new()),
        seed,
    );

    spawner.spawn(connection(controller, device_cfg)).ok();
    spawner.spawn(net_task(runner)).ok();
    spawner.spawn(mqtt_task(stack, device_cfg)).ok();

    // main() tidak melakukan apa-apa lagi - sensor_task, connection,
    // net_task, dan mqtt_task semuanya jalan independen sebagai task
    // embassy terpisah.
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>, device_cfg: &'static DeviceConfig) {
    info!("Start WiFi connection task");
    loop {
        if let WifiState::StaConnected = esp_wifi::wifi::wifi_state() {
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            Timer::after(Duration::from_millis(5000)).await;
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: device_cfg.ssid.as_str().try_into().unwrap(),
                password: device_cfg.password.as_str().try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            controller.start_async().await.unwrap();
        }
        match controller.connect_async().await {
            Ok(_) => info!("WiFi connected!"),
            Err(e) => {
                error!("Gagal connect WiFi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

/// Tunggu link up + dapat IP dari DHCP, lalu loop connect ke ThingsBoard
/// via MQTT dan publish `READING` terbaru tiap 5 detik. Reconnect otomatis
/// (jeda 3 detik) kalau koneksi putus - tidak pernah menyerah permanen.
/// Task ini TIDAK OTA (lihat catatan kepala file) - cuma publish telemetry
/// satu arah, tidak subscribe/proses perintah dari cloud.
#[embassy_executor::task]
async fn mqtt_task(
    stack: embassy_net::Stack<'static>,
    device_cfg: &'static DeviceConfig,
) -> ! {
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    info!("Menunggu IP dari DHCP...");
    loop {
        if let Some(cfg) = stack.config_v4() {
            info!("Dapat IP: {}", cfg.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    let mut rx_buffer = [0u8; 2048];
    let mut tx_buffer = [0u8; 2048];
    let mut recv_buffer = [0u8; 1024];
    let mut write_buffer = [0u8; 1024];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(15)));

        let tb_host = device_cfg.tb_host.as_str();
        let address = match stack.dns_query(tb_host, DnsQueryType::A).await.map(|a| a[0]) {
            Ok(addr) => addr,
            Err(e) => {
                error!("DNS lookup {} gagal: {:?}", tb_host, e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        };

        info!("Menghubungkan ke ThingsBoard {}:{}...", tb_host, TB_PORT);
        if socket.connect((address, TB_PORT)).await.is_err() {
            error!("TCP connect ke ThingsBoard gagal");
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        let mut mqtt_config = ClientConfig::new(
            rust_mqtt::client::client_config::MqttVersion::MQTTv5,
            CountingRng(20000),
        );
        mqtt_config.add_max_subscribe_qos(QualityOfService::QoS1);
        // ThingsBoard: access token dipakai sebagai MQTT username, password kosong.
        mqtt_config.add_username(device_cfg.tb_token.as_str());
        mqtt_config.add_client_id("coffee-enose-esp32s3");
        mqtt_config.max_packet_size = 1024;

        let mut client = MqttClient::<_, 5, _>::new(
            socket,
            &mut write_buffer,
            1024,
            &mut recv_buffer,
            1024,
            mqtt_config,
        );

        if let Err(e) = client.connect_to_broker().await {
            error!("MQTT connect gagal: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        info!("MQTT connected ke ThingsBoard");

        'session: loop {
            // NB: publish_tick/ping_tick dibuat ulang tiap putaran loop, jadi
            // kalau `incoming` sering resolve duluan, kedua timer ini jadi
            // restart terus dan bisa telat. Cukup aman untuk skeleton ini
            // karena ThingsBoard jarang push pesan ke device, tapi bukan
            // jaminan waktu yang presisi.
            let publish_tick = Timer::after(Duration::from_secs(5));
            // rust-mqtt tidak otomatis kirim PINGREQ (keep_alive default 60s
            // di ClientConfig) - kalau tidak ada trafik sama sekali dalam
            // waktu itu, broker bisa anggap koneksi mati. Publish tiap 5 detik
            // di atas sebenarnya sudah cukup untuk reset keep-alive di sisi
            // broker (spec MQTT: paket apa pun dari client menghitung), tapi
            // ping eksplisit ditambahkan sebagai jaring pengaman tambahan.
            let ping_tick = Timer::after(Duration::from_secs(20));
            let incoming = client.receive_message();

            match select3(publish_tick, ping_tick, incoming).await {
                Either3::First(()) => {
                    // Ambil bacaan sensor TERBARU dari sensor_task (independen
                    // dari loop ini). Kalau belum ada bacaan sama sekali
                    // (baru banget boot), skip publish ronde ini saja.
                    if let Some(r) = READING.try_take() {
                        let mut payload: HString<256> = HString::new();
                        let _ = write!(
                            payload,
                            "{{\"mq3\":{:.4},\"mq6\":{:.4},\"mq7\":{:.4},\"mq135\":{:.4},\
                             \"tgs2600\":{:.4},\"tgs2602\":{:.4},\"tgs2611\":{:.4},\"tgs2620\":{:.4},\
                             \"temp_c\":{},\"rh_pct\":{},\"dht_age_s\":{}}}",
                            r.mq3, r.mq6, r.mq7, r.mq135,
                            r.tgs2600, r.tgs2602, r.tgs2611, r.tgs2620,
                            r.temp_c, r.rh_pct, r.dht_age_s,
                        );

                        if let Err(e) = client
                            .send_message(
                                "v1/devices/me/telemetry",
                                payload.as_bytes(),
                                QualityOfService::QoS0,
                                false,
                            )
                            .await
                        {
                            error!("Publish telemetry gagal: {:?}", e);
                            break 'session;
                        }
                    }
                }
                Either3::Second(()) => {
                    if let Err(e) = client.send_ping().await {
                        error!("MQTT ping gagal: {:?}", e);
                        break 'session;
                    }
                }
                Either3::Third(Ok(_)) => {
                    // Task ini tidak OTA - pesan masuk (mis. shared attributes
                    // update) sengaja diabaikan, tidak ada subscribe aktif.
                }
                Either3::Third(Err(e)) => match e {
                    ReasonCode::NetworkError => {
                        error!("MQTT network error, reconnect...");
                        break 'session;
                    }
                    _ => error!("MQTT error saat receive: {:?}", e),
                },
            }
        }

        Timer::after(Duration::from_secs(3)).await;
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
/// "DATA,t_s,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620,temp_c,rh_pct,dht_age_s"
/// (kanal gas dalam VOLT hasil konversi, bukan raw ADC code lagi - lihat
/// catatan kepatuhan §17.5 di `GasReading`) DAN simpan ke `READING` buat
/// dikirim MQTT kalau connect (lihat `mqtt_task`). Task ini TIDAK
/// bergantung sama sekali ke WiFi/MQTT - tetap jalan walau keduanya gagal,
/// supaya logging lokal per-percobaan (lihat tools/record_sample.ps1)
/// tidak ikut kena dampak masalah jaringan/cloud.
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
        let mq6 = raw_to_volts(block!(adc_a.read(channel::SingleA0)).unwrap_or(0));
        let mq135 = raw_to_volts(block!(adc_a.read(channel::SingleA1)).unwrap_or(0));
        let mq3 = raw_to_volts(block!(adc_a.read(channel::SingleA2)).unwrap_or(0));
        let mq7 = raw_to_volts(block!(adc_a.read(channel::SingleA3)).unwrap_or(0));
        let tgs2611 = raw_to_volts(block!(adc_b.read(channel::SingleA0)).unwrap_or(0));
        let tgs2602 = raw_to_volts(block!(adc_b.read(channel::SingleA1)).unwrap_or(0));
        let tgs2600 = raw_to_volts(block!(adc_b.read(channel::SingleA2)).unwrap_or(0));
        let tgs2620 = raw_to_volts(block!(adc_b.read(channel::SingleA3)).unwrap_or(0));

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

        esp_println::println!(
            "DATA,{t_s},{mq3:.4},{mq6:.4},{mq7:.4},{mq135:.4},{tgs2600:.4},{tgs2602:.4},{tgs2611:.4},{tgs2620:.4},\
             {held_temp_c},{held_rh_pct},{dht_age_s}"
        );

        READING.signal(GasReading {
            mq3,
            mq6,
            mq7,
            mq135,
            tgs2600,
            tgs2602,
            tgs2611,
            tgs2620,
            temp_c: held_temp_c,
            rh_pct: held_rh_pct,
            dht_age_s,
        });

        t_s += 1;
        ticker.next().await;
    }
}
