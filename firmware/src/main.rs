#![no_std]
#![no_main]

// Firmware: ESP32-S3, baca array sensor gas via ADS1115 (I2C), kirim
// telemetry ke ThingsBoard Cloud via MQTT.
//
// STATUS: WiFi + MQTT telemetry sedang divalidasi build-nya di hardware.
// OTA firmware update (ThingsBoard OTA package, protokol v2/fw/...) untuk
// SEMENTARA DIMATIKAN — hanya deteksi + log, belum benar-benar menulis ke
// flash — karena crate esp-hal-ota versi terbaru butuh esp-hal generasi
// lebih baru dari yang didukung esp-wifi saat ini (konflik versi). Aktifkan
// lagi setelah versi esp-wifi/esp-hal-ota searah generasinya.
//
// WiFi & ThingsBoard bisa diganti TANPA reflash: colok USB, buka serial
// monitor, tekan 'c'+Enter dalam 3 detik di awal boot untuk masuk wizard
// konfigurasi (tersimpan di flash, lihat src/config.rs). Kalau tidak ada
// input, dipakai config tersimpan sebelumnya atau default dari environment
// variable saat build.
//
// Array 8 sensor gas (MQ-3, MQ-6, MQ-7, MQ-135, TGS2600, TGS2602, TGS2611,
// TGS2620) dibaca lewat DUA modul ADS1115 di bus I2C yang sama (alamat 0x48
// default/ADDR->GND untuk 4 sensor pertama, 0x49/ADDR->VDD untuk 4 sensor
// kedua) - lihat sensor_task. INI ASUMSI STANDAR, BELUM DIVERIFIKASI KE
// HARDWARE ASLI karena rangkaiannya belum dirakit saat kode ini ditulis.
// Begitu wiring fisik sudah jadi, cek ulang: alamat I2C tiap modul (pin
// ADDR), urutan sensor per channel A0-A3, dan FullScaleRange sesuai rentang
// tegangan output masing-masing sensor.
//
// TODO sebelum dipakai di hardware:
//  - VERIFIKASI wiring 2x ADS1115 di atas begitu hardware dirakit (alamat
//    I2C, urutan sensor per channel).
//  - Sesuaikan pin I2C (SDA/SCL) untuk ADS1115 dengan wiring board Anda.
//  - Sesuaikan FullScaleRange ADS1115 dengan rentang tegangan output sensor.
//  - Set env var SSID, PASSWORD, TB_HOST, TB_TOKEN sebelum build (dipakai
//    sebagai default kalau belum pernah disimpan config lewat wizard serial;
//    lihat firmware/README.md).
//  - Verifikasi method Uart (`read_bytes`/`write_bytes`) terhadap versi
//    esp-hal yang benar-benar ter-resolve - ini juga belum pernah dicompile.
//  - Verifikasi nama method rust-mqtt (`subscribe_to_topic`,
//    `receive_message`) terhadap versi crate yang benar-benar ter-resolve —
//    API crate ini bisa berubah antar versi.
//  - Di ThingsBoard, set algoritma checksum OTA package ke CRC32 (parser
//    checksum di bawah ini hanya menangani CRC32 hex string).

mod config;

use core::fmt::Write as _;

use ads1x1x::{channel, Ads1x1x, FullScaleRange, TargetAddr};
use core::cell::RefCell;
use embedded_hal_bus::i2c::RefCellDevice;
use config::DeviceConfig;
use embassy_executor::Spawner;
use embassy_futures::select::{select3, Either3};
use embassy_net::{
    dns::DnsQueryType,
    tcp::TcpSocket,
    Config as EmbassyNetConfig, Runner, StackResources,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    i2c::master::{Config as I2cConfig, I2c},
    rng::Rng,
    timer::timg::TimerGroup,
    uart::{Config as UartConfig, Uart},
};
use esp_storage::FlashStorage;
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

const OTA_CHUNK_SIZE: usize = 4096;

#[derive(Clone, Copy)]
struct GasReading {
    mq3: i16,
    mq6: i16,
    mq7: i16,
    mq135: i16,
    tgs2600: i16,
    tgs2602: i16,
    tgs2611: i16,
    tgs2620: i16,
}

// Jembatan antara sensor_task (baca ADS1115 tiap 1 detik, jalan terus tidak
// peduli status WiFi/MQTT) dan loop MQTT di main() yang publish tiap 5 detik.
// Pakai Signal (bukan Channel) karena yang dibutuhkan cuma nilai TERBARU,
// bukan antrian semua history.
static READING: Signal<CriticalSectionRawMutex, GasReading> = Signal::new();

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write($val);
        x
    }};
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

    // I2C untuk ADS1115 (array sensor gas). TODO: sesuaikan pin.
    let i2c = I2c::new(peripherals.I2C0, I2cConfig::default())
        .unwrap()
        .with_sda(peripherals.GPIO8)
        .with_scl(peripherals.GPIO9);

    // Wajib diinisialisasi SEBELUM pemakaian Timer::after().await pertama
    // (termasuk yang dipakai wizard konfigurasi di bawah) - kalau tidak,
    // panic "schedule_wake called before esp_hal_embassy::init()".
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timg1.timer0);

    // Sensor gas jalan independen dari WiFi/MQTT - tetap baca & log ke serial
    // (baris "DATA,...") walau ThingsBoard/WiFi lagi bermasalah. Ini yang
    // dipakai untuk logging lokal per-sampel kopi ke laptop (lihat
    // tools/record_sample.ps1), sekaligus jadi sumber data buat dikirim ke
    // ThingsBoard kalau MQTT connect.
    spawner.spawn(sensor_task(i2c)).ok();

    // --- Konfigurasi WiFi/ThingsBoard: bisa diubah tanpa reflash lewat
    // wizard di serial monitor, tersimpan di flash. Kalau tidak ada input
    // dalam 3 detik, lanjut pakai config tersimpan (atau default dari
    // environment variable saat build kalau belum pernah disimpan). ---
    // NB: method sinkron `uart.read()` di esp-hal ternyata BLOCK sampai ada
    // byte masuk (bukan langsung balik 0 kalau FIFO kosong seperti dugaan
    // awal, terbukti lewat testing di hardware). Makanya pakai `.into_async()`
    // + `read_async()` (nama method async-nya beda dari yang sinkron, bukan
    // lewat trait `.read().await`), supaya bisa di-race dengan timeout tanpa
    // memblokir executor (termasuk sensor_task).
    let mut uart0 = Uart::new(peripherals.UART0, UartConfig::default())
        .unwrap()
        .into_async();
    let mut flash = FlashStorage::new();

    esp_println::println!(
        "Tekan 'c' lalu Enter dalam 3 detik untuk ubah WiFi/ThingsBoard (skip = lanjut biasa)..."
    );
    let trigger = embassy_time::with_timeout(Duration::from_secs(3), async {
        loop {
            let mut b = [0u8; 1];
            if uart0.read_async(&mut b).await.is_ok() && (b[0] == b'c' || b[0] == b'C') {
                return;
            }
        }
    })
    .await
    .is_ok();

    if trigger {
        esp_println::println!("");
        esp_println::println!("=== Wizard Konfigurasi WiFi/ThingsBoard ===");

        esp_println::println!("Nama WiFi (SSID): ");
        let ssid = read_line(&mut uart0).await;
        esp_println::println!("");

        esp_println::println!("Password WiFi: ");
        let password = read_line(&mut uart0).await;
        esp_println::println!("");

        esp_println::println!("Host ThingsBoard (mis. thingsboard.cloud): ");
        let tb_host = read_line(&mut uart0).await;
        esp_println::println!("");

        esp_println::println!("Access token ThingsBoard: ");
        let tb_token = read_line(&mut uart0).await;
        esp_println::println!("");

        let mut new_cfg = DeviceConfig {
            ssid: HString::new(),
            password: HString::new(),
            tb_host: HString::new(),
            tb_token: HString::new(),
        };
        let _ = new_cfg.ssid.push_str(&ssid.as_str()[..ssid.len().min(32)]);
        let _ = new_cfg
            .password
            .push_str(&password.as_str()[..password.len().min(64)]);
        let _ = new_cfg
            .tb_host
            .push_str(&tb_host.as_str()[..tb_host.len().min(64)]);
        let _ = new_cfg
            .tb_token
            .push_str(&tb_token.as_str()[..tb_token.len().min(32)]);

        config::save(&mut flash, &new_cfg);
        esp_println::println!("Config tersimpan ke flash. Reboot...");
        Timer::after(Duration::from_millis(200)).await;
        esp_hal::system::software_reset();
    }

    let device_cfg: &'static DeviceConfig = mk_static!(
        DeviceConfig,
        config::load(&mut flash).unwrap_or_else(DeviceConfig::from_build_env)
    );

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

    let mut rx_buffer = [0u8; 4096];
    let mut tx_buffer = [0u8; 4096];
    let mut recv_buffer = [0u8; OTA_CHUNK_SIZE + 256];
    let mut write_buffer = [0u8; OTA_CHUNK_SIZE + 256];

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
        mqtt_config.max_packet_size = (OTA_CHUNK_SIZE + 256) as u32;

        let mut client = MqttClient::<_, 5, _>::new(
            socket,
            &mut write_buffer,
            OTA_CHUNK_SIZE + 256,
            &mut recv_buffer,
            OTA_CHUNK_SIZE + 256,
            mqtt_config,
        );

        if let Err(e) = client.connect_to_broker().await {
            error!("MQTT connect gagal: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        info!("MQTT connected ke ThingsBoard");

        if let Err(e) = client.subscribe_to_topic("v1/devices/me/attributes").await {
            error!("Subscribe shared attributes gagal: {:?}", e);
        }

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
                    if let Some(reading) = READING.try_take() {
                        let mut payload: HString<192> = HString::new();
                        let _ = write!(
                            payload,
                            "{{\"mq3\":{},\"mq6\":{},\"mq7\":{},\"mq135\":{},\"tgs2600\":{},\"tgs2602\":{},\"tgs2611\":{},\"tgs2620\":{}}}",
                            reading.mq3,
                            reading.mq6,
                            reading.mq7,
                            reading.mq135,
                            reading.tgs2600,
                            reading.tgs2602,
                            reading.tgs2611,
                            reading.tgs2620
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
                Either3::Third(Ok((topic, payload))) => {
                    if topic == "v1/devices/me/attributes" {
                        if let Ok(text) = core::str::from_utf8(payload) {
                            if let Some(update) = parse_fw_update(text) {
                                info!(
                                    "OTA package terdeteksi: {} v{} ({} bytes) - fitur OTA \
                                     belum diaktifkan di build ini (lihat TODO OTA di firmware/README.md)",
                                    update.title, update.version, update.size
                                );
                                // TODO OTA: dulu ada implementasi tarik firmware chunk demi
                                // chunk (protokol v2/fw/...) dan tulis ke partisi OTA lewat
                                // esp-hal-ota, tapi crate itu butuh esp-hal generasi lebih
                                // baru dari yang dipakai esp-wifi saat ini sehingga bentrok
                                // versi. Aktifkan lagi setelah WiFi+MQTT telemetry di bawah
                                // ini terbukti jalan di hardware, dan versi esp-wifi/esp-hal-ota
                                // sudah searah generasinya.
                            }
                        }
                    }
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

/// Baca satu baris teks dari UART (blocking), sampai user tekan Enter.
/// Mendukung backspace sederhana. Dipakai wizard konfigurasi WiFi/ThingsBoard.
async fn read_line(uart: &mut Uart<'_, esp_hal::Async>) -> HString<64> {
    let mut buf: HString<64> = HString::new();
    loop {
        let mut b = [0u8; 1];
        if uart.read_async(&mut b).await.is_err() {
            continue;
        }
        match b[0] {
            b'\r' | b'\n' => {
                if !buf.is_empty() {
                    break;
                }
            }
            8 | 127 => {
                buf.pop();
            }
            byte => {
                let _ = uart.write_async(&[byte]).await;
                let _ = buf.push(byte as char);
            }
        }
    }
    buf
}

struct FwUpdate<'a> {
    title: &'a str,
    version: &'a str,
    size: u32,
    checksum_crc32: u32,
}

/// Parser JSON minimal (tanpa alokasi/serde) khusus untuk payload shared
/// attributes ThingsBoard saat OTA package di-assign ke device. Asumsi
/// checksum algorithm = CRC32 (hex string).
fn parse_fw_update(json: &str) -> Option<FwUpdate<'_>> {
    let title = extract_str_field(json, "fw_title")?;
    let version = extract_str_field(json, "fw_version")?;
    let size = extract_u32_field(json, "fw_size")?;
    let checksum_hex = extract_str_field(json, "fw_checksum")?;
    let checksum_crc32 = u32::from_str_radix(checksum_hex, 16).ok()?;
    Some(FwUpdate {
        title,
        version,
        size,
        checksum_crc32,
    })
}

fn extract_str_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let mut pat: HString<32> = HString::new();
    let _ = write!(pat, "\"{key}\":\"");
    let start = json.find(pat.as_str())? + pat.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn extract_u32_field(json: &str, key: &str) -> Option<u32> {
    let mut pat: HString<32> = HString::new();
    let _ = write!(pat, "\"{key}\":");
    let start = json.find(pat.as_str())? + pat.len();
    let rest = &json[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
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

/// Baca 8 sensor gas (2x ADS1115 di bus I2C yang sama, alamat 0x48 & 0x49)
/// tiap 1 detik, cetak ke serial (baris
/// "DATA,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620") dan simpan ke
/// `READING` buat dikirim MQTT kalau connect. Task ini TIDAK bergantung sama
/// sekali ke WiFi/MQTT - tetap jalan walau keduanya gagal, supaya logging
/// lokal per-percobaan (lihat tools/record_sample.ps1) tidak ikut kena
/// dampak masalah jaringan/cloud.
///
/// PERINGATAN: wiring 2 modul ADS1115 ini (alamat 0x48/0x49 lewat pin ADDR)
/// belum pernah diverifikasi ke hardware asli - sesuaikan begitu rangkaian
/// fisiknya jadi (lihat catatan TODO di kepala file).
#[embassy_executor::task]
async fn sensor_task(i2c: I2c<'static, esp_hal::Blocking>) {
    let i2c_bus = RefCell::new(i2c);

    // Modul #1 (ADDR->GND, alamat default 0x48): MQ-3, MQ-6, MQ-7, MQ-135.
    let mut adc_a = Ads1x1x::new_ads1115(RefCellDevice::new(&i2c_bus), TargetAddr::default());
    // Modul #2 (ADDR->VDD, alamat 0x49): TGS2600, TGS2602, TGS2611, TGS2620.
    let mut adc_b = Ads1x1x::new_ads1115(RefCellDevice::new(&i2c_bus), TargetAddr::Vdd);

    if adc_a.set_full_scale_range(FullScaleRange::Within4_096V).is_err() {
        warn!("Gagal set full scale range ADS1115 #1 / 0x48 (cek wiring/alamat I2C)");
    }
    if adc_b.set_full_scale_range(FullScaleRange::Within4_096V).is_err() {
        warn!("Gagal set full scale range ADS1115 #2 / 0x49 (cek wiring/alamat I2C)");
    }

    loop {
        let mq3 = block!(adc_a.read(channel::SingleA0)).unwrap_or(0);
        let mq6 = block!(adc_a.read(channel::SingleA1)).unwrap_or(0);
        let mq7 = block!(adc_a.read(channel::SingleA2)).unwrap_or(0);
        let mq135 = block!(adc_a.read(channel::SingleA3)).unwrap_or(0);
        let tgs2600 = block!(adc_b.read(channel::SingleA0)).unwrap_or(0);
        let tgs2602 = block!(adc_b.read(channel::SingleA1)).unwrap_or(0);
        let tgs2611 = block!(adc_b.read(channel::SingleA2)).unwrap_or(0);
        let tgs2620 = block!(adc_b.read(channel::SingleA3)).unwrap_or(0);

        esp_println::println!(
            "DATA,{mq3},{mq6},{mq7},{mq135},{tgs2600},{tgs2602},{tgs2611},{tgs2620}"
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
        });

        Timer::after(Duration::from_secs(1)).await;
    }
}
