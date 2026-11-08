#!/usr/bin/env bash
# Install pinned, SHA-256-verified models for the Linux Rust app.
set -euo pipefail

models=${DEX_MODELS:-${XDG_DATA_HOME:-$HOME/.local/share}/dex-voice/models}
asr='https://huggingface.co/handy-computer/parakeet-unified-en-0.6b-gguf/resolve/843b1b175ace04f5a89d379c79e2518c0d065b25'
tts='https://huggingface.co/gradium/phonon-31m-en/resolve/92741c5b07d085ff840e26fea8ce43110b87852f'
voices='https://huggingface.co/gradium/phonon-voices/resolve/0ee17205f254b5abc43b266c8a0f215565c352f7'

install_file() {
    local relative=$1 expected=$2 url=$3 target="$models/$1" partial
    partial="$target.download"
    mkdir -p "$(dirname "$target")"
    if [[ -f "$target" ]]; then
        if [[ $(sha256sum "$target" | cut -d ' ' -f 1) != "$expected" ]]; then
            printf 'Checksum mismatch: %s (leaving existing file untouched)\n' "$target" >&2
            return 1
        fi
        printf 'Verified: %s\n' "$relative"
        return
    fi
    printf 'Downloading: %s\n' "$relative"
    curl --fail --location --continue-at - --output "$partial" "$url"
    if [[ $(sha256sum "$partial" | cut -d ' ' -f 1) != "$expected" ]]; then
        printf 'Checksum mismatch: %s (download retained for inspection)\n' "$partial" >&2
        return 1
    fi
    mv -- "$partial" "$target"
    printf 'Verified: %s\n' "$relative"
}

install_file gguf/parakeet-unified-en-0.6b-F16.gguf \
    4a284b229bff9dc66aa00666fdcc2419c0f82a5adb5a36f7a48a4e0713c35492 \
    "$asr/parakeet-unified-en-0.6b-F16.gguf"
install_file phonon-31m-en/config.json \
    b5d0f88353638370d94035b7d73d7d6577c0554d78f8d7ecfd40490aae39ae85 \
    "$tts/config.json"
install_file phonon-31m-en/tokenizer.json \
    691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936 \
    "$tts/tokenizer.json"
install_file phonon-31m-en/model.q8.gguf \
    a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae \
    "$tts/model.q8.gguf"
install_file phonon-31m-en/default-voice.safetensors \
    dc537478eaadc1a5257fcda2e3501ab81e14c111a5d95a5838f350db2afd0f1a \
    "$tts/default-voice.safetensors"
install_file phonon-31m-en/voices/NbpkqMVS3CJeq2j8.safetensors \
    97c7f8c253413574406ab80dd23f49c3ebb2517163b00a9bf86b9c4cf43e7e23 \
    "$voices/voices/NbpkqMVS3CJeq2j8.safetensors"
install_file phonon-31m-en/voices/NIWIOv5vTi5eRMj8.safetensors \
    444378eb06b7c492b86e8fc7d13e4db1a3c0cf3e68891a960bf6f0a41a13ef35 \
    "$voices/voices/NIWIOv5vTi5eRMj8.safetensors"
printf 'Models installed in %s\n' "$models"
