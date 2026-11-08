# Pinned, verified model install for the Rust Windows app. No drivers are changed.
param([string]$Destination = (Join-Path $env:LOCALAPPDATA 'DexVoice\models'))
$ErrorActionPreference = 'Stop'

$files = @(
    @{
        Path = 'gguf/parakeet-unified-en-0.6b-F16.gguf'
        Url = 'https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/843b1b175ace04f5a89d379c79e2518c0d065b25/parakeet-unified-en-0.6b-F16.gguf'
        Hash = '4a284b229bff9dc66aa00666fdcc2419c0f82a5adb5a36f7a48a4e0713c35492'
    },
    @{
        Path = 'phonon-31m-en/config.json'
        Url = 'https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/config.json'
        Hash = 'b5d0f88353638370d94035b7d73d7d6577c0554d78f8d7ecfd40490aae39ae85'
    },
    @{
        Path = 'phonon-31m-en/tokenizer.json'
        Url = 'https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/tokenizer.json'
        Hash = '691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936'
    },
    @{
        Path = 'phonon-31m-en/model.q8.gguf'
        Url = 'https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/model.q8.gguf'
        Hash = 'a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae'
    },
    @{
        Path = 'phonon-31m-en/default-voice.safetensors'
        Url = 'https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f/default-voice.safetensors'
        Hash = 'dc537478eaadc1a5257fcda2e3501ab81e14c111a5d95a5838f350db2afd0f1a'
    }
)

foreach ($file in $files) {
    $target = Join-Path $Destination ($file.Path.Replace('/', [IO.Path]::DirectorySeparatorChar))
    $dir = Split-Path -Parent $target
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    if ((Test-Path $target) -and (Get-FileHash -Algorithm SHA256 $target).Hash -ieq $file.Hash) {
        Write-Host "Verified: $($file.Path)"
        continue
    }
    $partial = "$target.download"
    Write-Host "Downloading: $($file.Path)"
    Invoke-WebRequest -Uri $file.Url -OutFile $partial -MaximumRedirection 10
    if ((Get-FileHash -Algorithm SHA256 $partial).Hash -ine $file.Hash) {
        throw "SHA-256 mismatch: $($file.Path). Check the source and retry."
    }
    Move-Item -Force $partial $target
    Write-Host "Verified: $($file.Path)"
}
Write-Host "Models installed in $Destination"
