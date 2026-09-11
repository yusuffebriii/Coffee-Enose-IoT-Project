<#
Rekam data 8 sensor gas (MQ-3, MQ-6, MQ-7, MQ-135, TGS2600, TGS2602,
TGS2611, TGS2620 - lewat 2x ADS1115) dari ESP32-S3 ke file CSV, satu file
per percobaan. Firmware harus sudah ter-flash dan mencetak baris
"DATA,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620" ke serial (lihat
sensor_task di src/main.rs) - ini jalan independen dari status WiFi/ThingsBoard.

Skenario: 10x percobaan, masing-masing 100 gram kopi, direkam 300 detik.

Cara pakai (nama percobaan bebas, mis. sertakan nomor & jenis kopi):
    .\record_sample.ps1 -SampleName "arabika_gayo_percobaan_1"
    .\record_sample.ps1 -SampleName "robusta_lampung_percobaan_2" -Port COM5 -DurationSeconds 300

Kalau -Port tidak diisi, script otomatis cari port ESP32-S3 (nomor COM-nya
suka berubah-ubah tiap dicabut/dicolok ulang di Windows).

Tekan Ctrl+C buat berhenti kapan saja (durasi opsional, default tanpa batas).
Hasil disimpan ke 01_raw_data/<SampleName>_<timestamp>.csv
#>
param(
    [Parameter(Mandatory = $true)]
    [string]$SampleName,

    # Kosongkan supaya otomatis dideteksi
    [string]$Port = "",

    # 0 = rekam terus sampai Ctrl+C ditekan manual
    [int]$DurationSeconds = 0
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Port)) {
    # CH343 (VID_1A86) = kabel USB-UART bridge, native USB Espressif
    # (VID_303A) = port USB langsung ke chip. Salah satunya biasanya ESP32-S3.
    $candidate = Get-PnpDevice -Class Ports -PresentOnly |
        Where-Object { $_.InstanceId -match 'VID_1A86|VID_303A' } |
        Select-Object -First 1

    if (-not $candidate) {
        Write-Host "Tidak ketemu port ESP32-S3 otomatis. Port yang tersedia sekarang:"
        Get-PnpDevice -Class Ports -PresentOnly | Format-Table FriendlyName, InstanceId -AutoSize
        throw "Colok ESP32-S3 dulu, atau isi manual lewat -Port COMx"
    }

    if ($candidate.FriendlyName -match '\((COM\d+)\)') {
        $Port = $matches[1]
        Write-Host "Port terdeteksi otomatis: $Port ($($candidate.FriendlyName))"
    }
    else {
        throw "Gagal membaca nomor COM dari: $($candidate.FriendlyName). Isi manual lewat -Port COMx"
    }
}

$rawDataDir = Join-Path $PSScriptRoot "..\..\01_raw_data"
$rawDataDir = (Resolve-Path $rawDataDir).Path

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$safeSampleName = $SampleName -replace '[^a-zA-Z0-9_\-]', '_'
$outFile = Join-Path $rawDataDir "$safeSampleName`_$timestamp.csv"

"timestamp,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620" | Out-File -FilePath $outFile -Encoding utf8

$serialPort = New-Object System.IO.Ports.SerialPort $Port, 115200, ([System.IO.Ports.Parity]::None), 8, ([System.IO.Ports.StopBits]::One)
$serialPort.NewLine = "`n"
$serialPort.ReadTimeout = 2000
$serialPort.Open()

Write-Host "Merekam sampel '$SampleName' dari $Port"
Write-Host "Output: $outFile"
Write-Host "Tekan Ctrl+C untuk berhenti."
Write-Host ""

$count = 0
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

try {
    while ($true) {
        if ($DurationSeconds -gt 0 -and $stopwatch.Elapsed.TotalSeconds -ge $DurationSeconds) {
            break
        }

        $line = $null
        try {
            $line = $serialPort.ReadLine().Trim()
        }
        catch [System.TimeoutException] {
            continue
        }

        if ($line -and $line.StartsWith("DATA,")) {
            $ts = Get-Date -Format "o"
            $values = $line.Substring(5)
            "$ts,$values" | Out-File -FilePath $outFile -Encoding utf8 -Append
            $count++
            Write-Host "[$count] $ts,$values"
        }
    }
}
finally {
    $serialPort.Close()
    Write-Host ""
    Write-Host "Selesai. $count baris data tersimpan ke:"
    Write-Host "  $outFile"
}
