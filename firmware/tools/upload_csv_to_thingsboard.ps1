<#
Upload isi CSV hasil record_sample.ps1 (format:
timestamp,mq3,mq6,mq7,mq135,tgs2600,tgs2602,tgs2611,tgs2620) ke ThingsBoard
sebagai telemetry historis, pakai timestamp asli dari tiap baris CSV.
Ini dikirim langsung dari laptop lewat HTTP - tidak lewat ESP32.

Cara pakai:
    .\upload_csv_to_thingsboard.ps1 -CsvPath "..\..\01_raw_data\nama_file.csv" -AccessToken "MK5Mu4N2oxFV2NsDnA7u"
#>
param(
    [Parameter(Mandatory = $true)]
    [string]$CsvPath,

    [Parameter(Mandatory = $true)]
    [string]$AccessToken,

    [string]$TbHost = "thingsboard.cloud"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $CsvPath)) {
    throw "File tidak ditemukan: $CsvPath"
}

$rows = Import-Csv -Path $CsvPath
if ($rows.Count -eq 0) {
    throw "CSV kosong (cuma header, tidak ada baris data)."
}

Write-Host "Membaca $($rows.Count) baris dari $CsvPath..."

$payload = @()
foreach ($row in $rows) {
    $ts = [DateTimeOffset]::Parse($row.timestamp).ToUnixTimeMilliseconds()
    $payload += @{
        ts     = $ts
        values = @{
            mq3     = [int]$row.mq3
            mq6     = [int]$row.mq6
            mq7     = [int]$row.mq7
            mq135   = [int]$row.mq135
            tgs2600 = [int]$row.tgs2600
            tgs2602 = [int]$row.tgs2602
            tgs2611 = [int]$row.tgs2611
            tgs2620 = [int]$row.tgs2620
        }
    }
}

$json = $payload | ConvertTo-Json -Depth 5
$url = "https://$TbHost/api/v1/$AccessToken/telemetry"

Write-Host "Mengirim ke $url ..."
Invoke-RestMethod -Uri $url -Method Post -Body $json -ContentType "application/json"

Write-Host "Selesai. $($rows.Count) baris berhasil dikirim ke ThingsBoard."
Write-Host "Cek di ThingsBoard: device -> Latest telemetry, atau widget chart -> perbesar rentang waktu (History) supaya kelihatan datanya."
