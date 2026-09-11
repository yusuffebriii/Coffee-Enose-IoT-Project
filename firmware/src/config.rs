// Konfigurasi WiFi/ThingsBoard yang bisa diubah tanpa reflash, disimpan di
// flash internal ESP32-S3 (bukan di-hardcode ke binary saat compile).
//
// Prioritas nilai yang dipakai saat boot (lihat main.rs):
//  1. Kalau wifi_config.txt BERUBAH sejak build terakhir yang tersimpan di
//     flash (dideteksi lewat CONFIG_HASH dari build.rs) -> pakai nilai baru
//     dari wifi_config.txt, timpa apa pun yang ada di flash. Ini supaya
//     developer (yang punya toolchain, edit file + rebuild + reflash) tidak
//     perlu JUGA jalankan wizard tiap kali ganti WiFi.
//  2. Kalau wifi_config.txt SAMA seperti build sebelumnya (hash cocok), dan
//     ada config valid di flash -> pakai config di flash. Ini yang bikin
//     wizard (buat orang di lapangan tanpa toolchain) tetap persist antar
//     reboot/reset biasa, tidak ke-reset cuma karena reflash binary yang
//     identik (mis. setelah tambah fitur lain yang tidak sentuh WiFi).
//  3. Kalau belum ada apa-apa di flash sama sekali -> pakai wifi_config.txt.

use embedded_storage::{ReadStorage, Storage};
use esp_storage::FlashStorage;
use heapless::String as HString;

// Offset flash mentah, sengaja di luar semua partisi app/OTA di partitions.csv
// (ota_1 berakhir di 0x1a0000 + 0x180000 = 0x320000). Cuma butuh 1 sektor (4KB).
const CONFIG_FLASH_OFFSET: u32 = 0x320000;
const CONFIG_MAGIC: u32 = 0xC0FFEE02;
const BUF_LEN: usize = 4 + 4 + (1 + 32) + (1 + 64) + (1 + 64) + (1 + 32); // 204 bytes

pub struct DeviceConfig {
    pub ssid: HString<32>,
    pub password: HString<64>,
    pub tb_host: HString<64>,
    pub tb_token: HString<32>,
}

impl DeviceConfig {
    /// Nilai default yang di-set lewat environment variable saat build
    /// (dibaca build.rs dari wifi_config.txt).
    pub fn from_build_env() -> Self {
        let mut cfg = DeviceConfig {
            ssid: HString::new(),
            password: HString::new(),
            tb_host: HString::new(),
            tb_token: HString::new(),
        };
        let _ = cfg.ssid.push_str(env!("SSID"));
        let _ = cfg.password.push_str(env!("PASSWORD"));
        let _ = cfg.tb_host.push_str(env!("TB_HOST"));
        let _ = cfg.tb_token.push_str(env!("TB_TOKEN"));
        cfg
    }
}

/// Hash isi wifi_config.txt pas build ini (dari build.rs). Dipakai buat
/// deteksi apakah file itu berubah sejak config terakhir disimpan ke flash.
pub fn current_build_hash() -> u32 {
    env!("CONFIG_HASH").parse().unwrap_or(0)
}

fn write_field(buf: &mut [u8], pos: &mut usize, s: &str, max_len: usize) {
    let len = s.len().min(max_len);
    buf[*pos] = len as u8;
    buf[*pos + 1..*pos + 1 + len].copy_from_slice(&s.as_bytes()[..len]);
    *pos += 1 + max_len;
}

fn read_field<'a>(buf: &'a [u8], pos: &mut usize, max_len: usize) -> Option<&'a str> {
    let len = buf[*pos] as usize;
    if len > max_len {
        return None;
    }
    let s = core::str::from_utf8(&buf[*pos + 1..*pos + 1 + len]).ok()?;
    *pos += 1 + max_len;
    Some(s)
}

/// Baca config + hash wifi_config.txt yang tersimpan dari flash. `None`
/// kalau belum pernah disimpan (magic tidak cocok / format lama).
pub fn load(flash: &mut FlashStorage) -> Option<(DeviceConfig, u32)> {
    let mut buf = [0u8; BUF_LEN];
    flash.read(CONFIG_FLASH_OFFSET, &mut buf).ok()?;

    let magic = u32::from_le_bytes(buf[0..4].try_into().unwrap());
    if magic != CONFIG_MAGIC {
        return None;
    }
    let saved_hash = u32::from_le_bytes(buf[4..8].try_into().unwrap());

    let mut pos = 8;
    let ssid = read_field(&buf, &mut pos, 32)?;
    let password = read_field(&buf, &mut pos, 64)?;
    let tb_host = read_field(&buf, &mut pos, 64)?;
    let tb_token = read_field(&buf, &mut pos, 32)?;

    let mut cfg = DeviceConfig {
        ssid: HString::new(),
        password: HString::new(),
        tb_host: HString::new(),
        tb_token: HString::new(),
    };
    cfg.ssid.push_str(ssid).ok()?;
    cfg.password.push_str(password).ok()?;
    cfg.tb_host.push_str(tb_host).ok()?;
    cfg.tb_token.push_str(tb_token).ok()?;
    Some((cfg, saved_hash))
}

/// Simpan config + hash wifi_config.txt saat ini ke flash. Dipanggil baik
/// dari wizard konfigurasi via serial (dengan `current_build_hash()`) maupun
/// otomatis di boot saat build.rs mendeteksi wifi_config.txt berubah.
pub fn save(flash: &mut FlashStorage, cfg: &DeviceConfig, build_hash: u32) {
    let mut buf = [0u8; BUF_LEN];
    buf[0..4].copy_from_slice(&CONFIG_MAGIC.to_le_bytes());
    buf[4..8].copy_from_slice(&build_hash.to_le_bytes());
    let mut pos = 8;
    write_field(&mut buf, &mut pos, cfg.ssid.as_str(), 32);
    write_field(&mut buf, &mut pos, cfg.password.as_str(), 64);
    write_field(&mut buf, &mut pos, cfg.tb_host.as_str(), 64);
    write_field(&mut buf, &mut pos, cfg.tb_token.as_str(), 32);
    let _ = flash.write(CONFIG_FLASH_OFFSET, &buf);
}
