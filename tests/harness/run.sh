#!/usr/bin/env bash
# Integration test against a real libobs (headless, OpenGL via Xvfb/llvmpipe).
#
#   OBS_SRC=/path/to/obs-studio   (source tree, libobs + libobs-opengl built in $OBS_SRC/build)
#   tests/harness/run.sh
#
# Renders every filter mode into tests/harness/out/*.ppm and fails on empty
# frames. Build libobs headless e.g. with:
#   cmake -S $OBS_SRC -B $OBS_SRC/build -G Ninja -DENABLE_FRONTEND=OFF -DENABLE_SCRIPTING=OFF \
#         -DENABLE_PLUGINS=OFF -DENABLE_PULSEAUDIO=OFF -DENABLE_WAYLAND=OFF
#   cmake --build $OBS_SRC/build --target libobs libobs-opengl
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
: "${OBS_SRC:?set OBS_SRC to the obs-studio source tree}"
B="$OBS_SRC/build"

cargo build --release --manifest-path "$ROOT/Cargo.toml"
mkdir -p "$HERE/out" "$HERE/plug"
cp "$ROOT/target/release/libareadox_blur.so" "$HERE/plug/areadox-blur.so"
gcc -O1 -g "$HERE/harness.c" -o "$HERE/harness" -I"$OBS_SRC/libobs" -I"$B/config" -L"$B/libobs" -lobs -lX11
python3 - "$HERE/out/mask.png" <<'PY'
import sys, math
from PIL import Image
im = Image.new('RGBA', (640, 360))
px = im.load()
for y in range(360):
    for x in range(640):
        v = int(max(0, min(1, 1 - math.hypot(x - 320, y - 180) / 200)) * 255)
        px[x, y] = (v, v, v, 255)
im.save(sys.argv[1])
PY
if [[ -z "${DISPLAY:-}" ]]; then
    Xvfb :99 -screen 0 1280x720x24 >/dev/null 2>&1 &
    XVFB=$!
    trap 'kill $XVFB' EXIT
    export DISPLAY=:99
    sleep 1
fi
export LD_LIBRARY_PATH="$B/libobs-opengl:$B/libobs" LIBOBS_DATA="$OBS_SRC/libobs/data/"
"$HERE/harness" "$HERE/plug/areadox-blur.so" "$ROOT/data" "$HERE/out"
