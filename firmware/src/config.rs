// Konfigurasi WiFi/ThingsBoard, dibaca dari environment variable saat
// compile (di-set otomatis oleh build.rs dari file wifi_config.txt - lihat
// firmware/README.md). Sengaja TIDAK ada penyimpanan ke flash/wizard serial
// seperti versi sebelumnya di riwayat git: fitur "ganti WiFi tanpa reflash"
// itu perlu menulis raw flash di offset tetap, yang cuma aman kalau ada
// partition table custom yang menjamin offset itu di luar partisi app -
// tanpa hardware untuk diuji ulang, lebih aman ganti config lewat
// wifi_config.txt + reflash saja dulu. Bisa ditambah lagi nanti.

use heapless::String as HString;

pub struct DeviceConfig {
    pub ssid: HString<32>,
    pub password: HString<64>,
    pub tb_host: HString<64>,
    pub tb_token: HString<32>,
}

impl DeviceConfig {
    /// Nilai dari environment variable saat build (lihat build.rs).
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
