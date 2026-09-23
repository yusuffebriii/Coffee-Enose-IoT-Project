// Baca firmware/wifi_config.txt (format KEY=VALUE sederhana) dan set jadi
// environment variable saat compile, supaya env!("SSID") dkk di
// src/config.rs bisa baca nilainya - user cukup edit wifi_config.txt
// langsung, tidak perlu paham env var/PowerShell.

use std::fs;
use std::path::Path;

const REQUIRED_KEYS: [&str; 4] = ["SSID", "PASSWORD", "TB_HOST", "TB_TOKEN"];

fn main() {
    let path = Path::new("wifi_config.txt");
    println!("cargo:rerun-if-changed=wifi_config.txt");

    let content = fs::read_to_string(path).unwrap_or_else(|_| {
        panic!(
            "\n\nfirmware/wifi_config.txt tidak ditemukan.\n\
             Copy firmware/wifi_config.example.txt jadi firmware/wifi_config.txt, \
             lalu isi SSID/PASSWORD/TB_HOST/TB_TOKEN.\n\n"
        )
    });

    let mut found = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            println!("cargo:rustc-env={key}={value}");
            found.push(key.to_string());
        }
    }

    for required in REQUIRED_KEYS {
        if !found.iter().any(|k| k == required) {
            panic!(
                "\n\nKey '{required}' tidak ada di firmware/wifi_config.txt. \
                 Cek lagi isinya, bandingkan dengan wifi_config.example.txt.\n\n"
            );
        }
    }
}
