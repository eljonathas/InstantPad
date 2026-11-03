#!/usr/bin/env bash
# Prepares what the voice training needs, once: Python 3.12, PyTorch, RVC and its base models.
#
#   ./setup.sh          for an NVIDIA graphics card (CUDA 12.8), on Linux or WSL 2
#   ./setup.sh --cpu    for a computer without one: it works, but training takes hours
#
# Everything goes to $INSTANTPAD_VOICE_HOME, ~/instantpad-voice-training by default. On WSL keep
# it in the Linux file system, not under /mnt/c, where reading the data is many times slower.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
home=${INSTANTPAD_VOICE_HOME:-$HOME/instantpad-voice-training}
device=cuda
for argument in "$@"; do
  case $argument in
    --cpu) device=cpu ;;
    *) echo "Unknown option: $argument (the only one is --cpu)" >&2; exit 2 ;;
  esac
done

missing=()
for tool in uv ffmpeg; do
  command -v "$tool" > /dev/null || missing+=("$tool")
done
if ((${#missing[@]})); then
  echo "Missing: ${missing[*]}" >&2
  echo "  uv:     curl -LsSf https://astral.sh/uv/install.sh | sh" >&2
  echo "  ffmpeg: sudo apt install ffmpeg" >&2
  exit 1
fi

if [[ $device == cuda ]] && ! command -v nvidia-smi > /dev/null; then
  echo "No NVIDIA driver found (nvidia-smi). On WSL, install the NVIDIA driver on Windows," >&2
  echo "not inside Linux. Or run ./setup.sh --cpu to train without the graphics card." >&2
  exit 1
fi

mkdir -p "$home"
venv=$home/venv
[[ -x $venv/bin/python ]] || uv venv --python 3.12 "$venv"

# RVC asks for PyTorch below 2.8; 2.7.1 is the version it was checked with.
uv pip install --python "$venv/bin/python" \
  --index-url "https://download.pytorch.org/whl/$([[ $device == cuda ]] && echo cu128 || echo cpu)" \
  torch==2.7.1 torchaudio==2.7.1
uv pip install --python "$venv/bin/python" -r "$here/requirements.lock"

"$venv/bin/python" "$here/voicekit.py" setup

echo
echo "Ready. Next: $venv/bin/python $here/voicekit.py all --name feminine"
