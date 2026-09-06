// Konfigurasi WiFi/ThingsBoard yang bisa diubah tanpa reflash, disimpan di
// flash internal ESP32-S3 (bukan di-hardcode ke binary saat compile).
//
// Kalau ada config valid tersimpan di flash, itu yang dipakai. Kalau tidak
// ada (belum pernah disimpan / flash kosong), fallback ke nilai default yang
// di-set lewat environment variable saat build (SSID, PASSWORD, TB_HOST,
// TB_TOKEN) - lihat firmware/README.md.

use embedded_storage::{ReadStorage, Storage};
use esp_storage::FlashStorage;
use heapless::String as HString;

// Offset flash mentah, sengaja di luar semua partisi app/OTA di partitions.csv
// (ota_1 berakhir di 0x1a0000 + 0x180000 = 0x320000). Cuma butuh 1 sektor (4KB).
const CONFIG_FLASH_OFFSET: u32 = 0x320000;
const CONFIG_MAGIC: u32 = 0xC0FFEE01;
const BUF_LEN: usize = 4 + (1 + 32) + (1 + 64) + (1 + 64) + (1 + 32); // 200 bytes

pub struct DeviceConfig {
    pub ssid: HString<32>,
    pub password: HString<64>,
    pub tb_host: HString<64>,
    pub tb_token: HString<32>,
}

impl DeviceConfig {
    /// Nilai default yang di-set lewat environment variable saat build.
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

/// Baca config dari flash. `None` kalau belum pernah disimpan (magic tidak cocok).
pub fn load(flash: &mut FlashStorage) -> Option<DeviceConfig> {
    let mut buf = [0u8; BUF_LEN];
    flash.read(CONFIG_FLASH_OFFSET, &mut buf).ok()?;

    let magic = u32::from_le_bytes(buf[0..4].try_into().unwrap());
    if magic != CONFIG_MAGIC {
        return None;
    }

    let mut pos = 4;
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
    Some(cfg)
}

/// Simpan config baru ke flash (dipanggil dari wizard konfigurasi via serial).
pub fn save(flash: &mut FlashStorage, cfg: &DeviceConfig) {
    let mut buf = [0u8; BUF_LEN];
    buf[0..4].copy_from_slice(&CONFIG_MAGIC.to_le_bytes());
    let mut pos = 4;
    write_field(&mut buf, &mut pos, cfg.ssid.as_str(), 32);
    write_field(&mut buf, &mut pos, cfg.password.as_str(), 64);
    write_field(&mut buf, &mut pos, cfg.tb_host.as_str(), 64);
    write_field(&mut buf, &mut pos, cfg.tb_token.as_str(), 32);
    let _ = flash.write(CONFIG_FLASH_OFFSET, &buf);
}
