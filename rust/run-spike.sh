#!/bin/sh
set -eu
cd "$(dirname "$0")"
export VULKAN_SDK="${VULKAN_SDK:-$PWD/../build/third_party/Vulkan-Headers}"
export CMAKE_PREFIX_PATH="${CMAKE_PREFIX_PATH:-$PWD/../build/third_party/spirv-prefix:$VULKAN_SDK}"
export CXXFLAGS="${CXXFLAGS:-} -I$PWD/../build/third_party/SPIRV-Headers/include"
export CFLAGS="${CFLAGS:-} -I$PWD/../build/third_party/SPIRV-Headers/include"
exec cargo run --release -p dex-asr-spike -- "$@"
