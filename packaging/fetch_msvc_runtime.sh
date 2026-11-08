#!/bin/sh
# Fetch the Microsoft Visual C++ runtime DLLs used by the portable Windows build.
# Microsoft permits redistribution of these files as part of an application.
set -eu
cd "$(dirname "$0")/.."
mkdir -p build/msvc build/windows-downloads
wheel=build/windows-downloads/msvc_runtime.whl
if [ ! -f "$wheel" ]; then
    curl --fail --location --output "$wheel" \
        "https://files.pythonhosted.org/packages/c2/6e/7ba5696e9d8c1bcb01352f06e441497c308e84f75aba598400646ab31ca7/msvc_runtime-14.51.36231-cp313-cp313-win_amd64.whl"
fi
unzip -j -o "$wheel" \
    'msvc_runtime-14.51.36231.data/data/msvcp140*.dll' \
    'msvc_runtime-14.51.36231.data/data/concrt140.dll' \
    'msvc_runtime-14.51.36231.data/data/Scripts/vcruntime140.dll' \
    'msvc_runtime-14.51.36231.data/data/Scripts/vcruntime140_1.dll' \
    'msvc_runtime-14.51.36231.data/data/vcruntime140_threads.dll' \
    -d build/msvc >/dev/null
ls -l build/msvc
