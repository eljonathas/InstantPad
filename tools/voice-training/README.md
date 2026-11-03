# Training a voice

*[Leia em português](README.pt-BR.md)*

These scripts train an AI voice with [RVC](https://github.com/RVC-Project/Retrieval-based-Voice-Conversion-WebUI)
and export it the way InstantPad loads it. They make the feminine voice of the app, and any
other voice from your own recordings.

They run on Linux and on Windows through WSL 2. With a recent NVIDIA graphics card the feminine
voice trains in about an hour; without one it trains on the processor, which works but takes
more than a day.

The app trains cloned voices by itself with this same kit, on Windows and Linux, with nothing to
install by hand ([ADR 0007](../../docs/adr/0007-training-voices-in-the-app.md)): pick a cloned
voice and click **Train a model...**. What follows is for training outside the app, such as the
feminine voice the app ships.

## Before you start

On WSL, install the NVIDIA driver on **Windows**, not inside Linux, and check that `nvidia-smi`
works in the Linux terminal. Then, in Linux:

```bash
sudo apt install ffmpeg
```

```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

## Training the feminine voice

```bash
tools/voice-training/setup.sh
```

```bash
~/instantpad-voice-training/venv/bin/python tools/voice-training/voicekit.py all --name feminine
```

`setup.sh --cpu` sets up a computer without a graphics card instead.

`all` runs three steps, which also run one by one, and each can be run again after a stop:

| Step | What it does |
| --- | --- |
| `dataset` | Downloads 400 MB of [Hi-Fi TTS](https://www.openslr.org/109/) and takes 40 minutes of reader 92 (`--minutes` changes it). |
| `train` | Cuts the speech into pieces, finds their pitch and words, and trains from RVC's base model: 200 epochs (`--epochs`), 8 pieces at once (`--batch`; lower it if the card runs out of memory). A stopped training resumes from its last save. |
| `export` | Writes the voice for the app to `~/instantpad-voice-training/voices/feminine/app` and checks the ONNX files against PyTorch. |

To hear it, convert a recording of yourself:

```bash
~/instantpad-voice-training/venv/bin/python tools/voice-training/voicekit.py try --name feminine me.wav
```

It writes `me.feminine.wav` next to it, with your pitch moved to the voice's. `--shift`
sets the semitones instead.

## Adding it to the app

In InstantPad, pick **Feminine AI** or a cloned voice, click **Add a trained model...** and choose the
`app` folder. On Windows, the folders of WSL are under **Linux** in the file dialog, or at
`\\wsl.localhost\<distribution>\home\<user>\instantpad-voice-training\voices\feminine\app`.

## Training a voice from your recordings

Put 10 to 40 minutes of one person talking in a folder, in any format ffmpeg reads, with no
music and as little noise as you can, then:

```bash
~/instantpad-voice-training/venv/bin/python tools/voice-training/voicekit.py all --name my-voice --folder ~/recordings --credit "Recordings of Ana"
```

## What the app gets

| File | What it is |
| --- | --- |
| `voice.json` | The voice: its sample rate, the pitch it usually speaks at, the files below with their SHA-256, and the credit. |
| `synthesizer.onnx` | The voice itself, about 110 MB. |
| `content.onnx` | HuBERT, which hears what is said. It is the same for every voice. |

`content.onnx` takes 16 kHz sound (`audio`, `[1, samples]`) and gives 50 frames a second of
768 values (`features`). The app repeats each frame to make 100 a second.

`synthesizer.onnx` speaks one chunk of the stream at 40 kHz. Its inputs:

| Input | Shape | What it is |
| --- | --- | --- |
| `phone` | `[1, frames, 768]` | The features of the whole window, 100 frames a second. |
| `phone_lengths` | `[1]` | `frames`. |
| `pitch` | `[1, frames]` | The pitch of each frame in 255 steps, from 50 to 1100 Hz on the mel scale; 1 is no pitch. |
| `pitchf` | `[1, played]` | The pitch in Hz of the last `played` frames, 0 when there is none. |
| `sid` | `[1]` | The speaker, from `voice.json`. |
| `noise` | `[1, 192, played + 24]` | Normal random values times 0.66666. |

`audio`, its output, is `[1, 1, played × 400]`: the last `played` frames of the window.

## What the app downloads

The app installs the same tools as `setup.sh`, on Windows too, from the files in
`environments/`: every wheel of each computer (Windows or Linux, with an NVIDIA card or the
processor) with its address, size and SHA-256. `pin.py` writes them from `requirements.txt` and
`requirements.lock`; run it again after changing either, and commit what changes:

```bash
uv run --no-project --python 3.12 tools/voice-training/pin.py
```

The app builds `voicekit.py` in, so a change to it reaches users with the next version.

## Sources and licences

- RVC, its scripts and its base models: MIT. The base models were trained on VCTK (CC BY 4.0).
- HuBERT base as RVC ships it: MIT.
- Hi-Fi TTS: CC BY 4.0, Evelina Bakhturina, Vitaly Lavrukhin, Boris Ginsburg and Yang Zhang,
  *Hi-Fi Multi-Speaker English TTS Dataset*, 2021. Its recordings come from LibriVox, in the
  public domain.

Every download is pinned by its size and SHA-256, and RVC by its commit, so what runs is what
was checked.
