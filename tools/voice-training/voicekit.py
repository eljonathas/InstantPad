#!/usr/bin/env python3
"""Trains an AI voice for InstantPad with RVC, and exports it the way the app loads it.

    voicekit.py setup                      RVC and its base models (setup.sh runs it)
    voicekit.py dataset --name feminine    40 minutes of a reader of Hi-Fi TTS
    voicekit.py train --name feminine      learns the voice
    voicekit.py export --name feminine     writes the voice for the app, and checks it
    voicekit.py try --name feminine a.wav  converts a recording, to hear the voice
    voicekit.py all --name feminine        dataset, train and export, one after the other

Every step can be run again: what is done already is kept. See README.md.

With --events before the step, it also writes one line of JSON for each step and each epoch,
after "@instantpad ", which is how InstantPad follows a training it runs.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import math
import os
import random
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import urllib.request
import zipfile
from pathlib import Path

HOME = Path(
    os.environ.get("INSTANTPAD_VOICE_HOME", Path.home() / "instantpad-voice-training")
)
RVC = HOME / "rvc"
DOWNLOADS = HOME / "downloads"
VOICES = HOME / "voices"
# Programs RVC calls by name, such as ffmpeg, when the computer has none.
TOOLS = HOME / "bin"

# RVC (MIT), the commit the scripts were checked with: Python 3.12, HuBERT through Transformers.
# It comes as an archive of that commit, so no git is needed.
RVC_REPOSITORY = "https://github.com/RVC-Project/Retrieval-based-Voice-Conversion-WebUI"
RVC_COMMIT = "81eed5e8f68b6bed1789f682fe78cdd324495afc"
RVC_ARCHIVE = (
    f"{RVC_REPOSITORY}/archive/{RVC_COMMIT}.zip",
    821_646,
    "c0427564fbeb584d1a4805a5e71b5897132a41b9029a0fa9f417497a6df13e43",
)

# The base models RVC trains from (MIT), at a fixed revision of their repository.
MODELS = "https://huggingface.co/lj1995/VoiceConversionWebUI/resolve/e6d0c1a17da07c33557852f9dfa2bd44cc75737d"
ASSETS = [
    # (path in the repository, where RVC wants it, size, SHA-256)
    (
        "hubert_base/config.json",
        "assets/hubert_base/config.json",
        1_492,
        "0346950779dfb7f9316fa74ed846e2b8a22a08eedfdc5387b73f327cb1a4a7cf",
    ),
    (
        "hubert_base/preprocessor_config.json",
        "assets/hubert_base/preprocessor_config.json",
        225,
        "7c1976a680fb7acc757cd36fb08eef878fa36c70b4c9d2d595df9c608bbbbf0e",
    ),
    (
        "hubert_base/pytorch_model.bin",
        "assets/hubert_base/pytorch_model.bin",
        189_206_711,
        "cc8c20f4b90a520757260197a3ff2505705a7adbd20ad9eeaa4e1a9b38442ef5",
    ),
    (
        "rmvpe.pt",
        "assets/rmvpe/rmvpe.pt",
        181_184_272,
        "6d62215f4306e3ca278246188607209f09af3dc77ed4232efdd069798c4ec193",
    ),
    (
        "pretrained_v2/f0G40k.pth",
        "assets/pretrained_v2/f0G40k.pth",
        73_106_273,
        "3b2c44035e782c4b14ddc0bede9e2f4a724d025cd073f736d4f43708453adfcb",
    ),
    (
        "pretrained_v2/f0D40k.pth",
        "assets/pretrained_v2/f0D40k.pth",
        142_875_703,
        "6b6ab091e70801b28e3f41f335f2fc5f3f35c75b39ae2628d419644ec2b0fa09",
    ),
]
# Silence RVC mixes into the training, so the voice learns to stay quiet in pauses.
MUTE = (
    "mute.zip",
    312_273,
    "ee948e85213e4ed2f2ba2f8dfcee810bfd0b63131d91450e920bbe1cbd0321d0",
)

# Hi-Fi TTS (CC BY 4.0): audiobooks of LibriVox (public domain) at 44.1 kHz. This part holds
# only reader 92, a woman, with 2 h 37 min of clean speech.
HIFI_TTS = {
    "url": "https://huggingface.co/datasets/MikhailT/hifi-tts/resolve/"
    "f0a934ddf4e8da0fc6c53fb42fda2c9ac8ed93df/data/"
    "train.clean-00034-of-00035-2e0606e24bdfa88a.parquet",
    "size": 400_177_601,
    "sha256": "4dfff23cb86d3020a3027a893555f5d92a1e4e28957a8df9315f7655232253f8",
    "speaker": "92",
}
HIFI_TTS_CREDIT = (
    "Hi-Fi TTS (Bakhturina et al., NVIDIA, CC BY 4.0), reader 92, from LibriVox recordings "
    "in the public domain"
)

SAMPLE_RATE = 40_000
HOP = 400  # samples of the voice per frame of features: 100 frames a second
CONTENT_RATE = 16_000
# Frames the flow sees before the part the app asks for, as the realtime converter of RVC does.
FLOW_CONTEXT = 24
# Each chunk of speech RVC learns from, in seconds.
SLICE_SECONDS = 3.7
FORMAT = 1


# ------------------------------------------------------------------------------------ helpers

# Set by --events.
EVENTS = False
EVENT_PREFIX = "@instantpad "
# The line RVC writes at the end of each epoch, in any of its languages: "====> Epoch: 12 ...".
EPOCH_LINE = re.compile(r"====>\D*(\d+)")


def event(**fields) -> None:
    """Tells the app where the work is, when it runs the kit."""
    if EVENTS:
        print(EVENT_PREFIX + json.dumps(fields), flush=True)


def end_with_input() -> None:
    """Ends this step, and every process it started, when its input closes. The app keeps the
    other end of the input open for as long as it lives, so the training does not outlive an app
    that crashed or was killed. The step is the leader of a group of its own there."""

    def wait() -> None:
        sys.stdin.buffer.read()  # returns only at the end of the input
        os.killpg(os.getpgrp(), signal.SIGKILL)

    threading.Thread(target=wait, daemon=True).start()


def say(message: str) -> None:
    print(f"\n==> {message}", flush=True)


def fail(message: str) -> None:
    event(error=message)
    sys.exit(f"error: {message}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def download(url: str, target: Path, size: int, digest: str) -> None:
    """Downloads `url` to `target` unless it is there already, resuming a download that stopped,
    and refuses a file that is not the one pinned here."""
    if target.exists() and target.stat().st_size == size and sha256(target) == digest:
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    part = target.with_name(target.name + ".part")
    have = part.stat().st_size if part.exists() else 0
    if have > size:
        part.unlink()
        have = 0
    if have < size:
        request = urllib.request.Request(
            url, headers={"Range": f"bytes={have}-"} if have else {}
        )
        with urllib.request.urlopen(request, timeout=60) as response:
            if have and response.status != 206:  # the server sends the whole file again
                have = 0
            with part.open("ab" if have else "wb") as file:
                done, shown = have, 0.0
                while block := response.read(1 << 20):
                    file.write(block)
                    done += len(block)
                    if time.monotonic() - shown > 2 or done == size:
                        shown = time.monotonic()
                        event(download=target.name, done=done, size=size)
                        print(
                            f"\r    {target.name}: {done / 1e6:.0f} of {size / 1e6:.0f} MB",
                            end="",
                            flush=True,
                        )
        print()
    if part.stat().st_size != size or sha256(part) != digest:
        part.unlink()
        fail(
            f"{url} is not the file this script knows; run it again to download it anew"
        )
    part.replace(target)


def python() -> str:
    return sys.executable


def run(arguments: list, log: Path | None = None, on_line=None) -> None:
    """Runs a script of RVC, `arguments` starting with its path, from RVC's folder. `on_line`
    hears each line it writes, which still reaches the terminal."""
    # The scripts import RVC's packages from its root, as its web interface runs them. -P keeps
    # the script's own folder out of the path: train/train.py would hide the package train.
    command = [python(), "-P", *(str(argument) for argument in arguments)]
    print("    $ " + " ".join(command[2:]), flush=True)
    environment = dict(
        os.environ,
        PYTHONUNBUFFERED="1",
        PYTHONUTF8="1",
        PYTHONPATH=str(RVC),
        PATH=str(TOOLS) + os.pathsep + os.environ.get("PATH", ""),
        # RVC writes in the language of the system, and the epochs are read in English.
        LANG="en_US.UTF-8",
        LC_ALL="en_US.UTF-8",
    )
    process = subprocess.Popen(
        command,
        cwd=RVC,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    for line in process.stdout:
        print(line, end="", flush=True)
        if on_line:
            on_line(line)
    process.wait()
    if process.returncode != 0:
        where = f" (see {log})" if log else ""
        fail(f"{Path(command[2]).name} stopped with code {process.returncode}{where}")


def using_rvc() -> None:
    """Makes RVC's own code importable, which needs its folder as the current one."""
    if not (RVC / "train" / "train.py").exists():
        fail("RVC is missing; run setup.sh first")
    os.chdir(RVC)
    if str(RVC) not in sys.path:
        sys.path.insert(0, str(RVC))


def cuda() -> bool:
    import torch

    return torch.cuda.is_available()


# --------------------------------------------------------------------------------------- setup


def setup(_: argparse.Namespace) -> None:
    event(step="setup")
    say(f"RVC at {RVC_COMMIT[:7]}")
    commit = RVC / ".commit"
    if not commit.exists() or commit.read_text(encoding="utf-8").strip() != RVC_COMMIT:
        url, size, digest = RVC_ARCHIVE
        archive = DOWNLOADS / f"rvc-{RVC_COMMIT[:7]}.zip"
        download(url, archive, size, digest)
        unpack_rvc(archive)
        commit.write_text(RVC_COMMIT + "\n", encoding="utf-8")

    say("Base models of RVC (about 590 MB)")
    for source, target, size, digest in ASSETS:
        download(f"{MODELS}/{source}", RVC / target, size, digest)
    name, size, digest = MUTE
    download(f"{MODELS}/{name}", DOWNLOADS / name, size, digest)
    if not (RVC / "logs" / "mute").exists():
        with zipfile.ZipFile(DOWNLOADS / name) as archive:
            archive.extractall(RVC / "logs")

    if not shutil.which("ffmpeg") and not any(TOOLS.glob("ffmpeg*")):
        # RVC decodes the recordings with the ffmpeg program when it trains on the processor.
        import imageio_ffmpeg

        TOOLS.mkdir(parents=True, exist_ok=True)
        program = "ffmpeg.exe" if os.name == "nt" else "ffmpeg"
        shutil.copy(imageio_ffmpeg.get_ffmpeg_exe(), TOOLS / program)

    import torch

    if torch.cuda.is_available():
        say(f"Graphics card: {torch.cuda.get_device_name(0)}")
    else:
        say("No graphics card: training will run on the processor, which takes hours")


def unpack_rvc(archive: Path) -> None:
    """Writes RVC's files over its folder, which keeps the models and trainings already there."""
    with zipfile.ZipFile(archive) as files:
        for entry in files.infolist():
            # Every file is under one folder named after the commit.
            relative = Path(*Path(entry.filename).parts[1:])
            if not relative.parts or entry.is_dir():
                continue
            target = RVC / relative
            if RVC.resolve() not in target.resolve().parents:
                fail(f"{archive} has a file outside its folder: {entry.filename}")
            target.parent.mkdir(parents=True, exist_ok=True)
            with files.open(entry) as source, target.open("wb") as copy:
                shutil.copyfileobj(source, copy)


# ------------------------------------------------------------------------------------- dataset


def dataset_folder(name: str) -> Path:
    return VOICES / name / "dataset"


def dataset(arguments: argparse.Namespace) -> None:
    folder = dataset_folder(arguments.name)
    if arguments.folder:
        source = Path(arguments.folder).expanduser().resolve()
        if not source.is_dir():
            fail(f"{source} is not a folder")
        if folder.exists() or folder.is_symlink():
            if folder.is_symlink() or not any(folder.iterdir()):
                folder.unlink() if folder.is_symlink() else folder.rmdir()
            else:
                fail(
                    f"{folder} already has recordings; remove it to use another folder"
                )
        folder.parent.mkdir(parents=True, exist_ok=True)
        folder.symlink_to(source, target_is_directory=True)
        write_credit(arguments.name, arguments.credit or f"Recordings of {source.name}")
        say(f"The voice learns from the recordings in {source}")
        return

    # Written last, so recordings cut short by a stop are taken again.
    done = VOICES / arguments.name / "dataset.done"
    if done.exists() and folder.is_dir():
        say(f"The recordings are ready in {folder}")
        return
    say("Hi-Fi TTS, reader 92 (400 MB)")
    shard = DOWNLOADS / "hifi-tts-92.parquet"
    download(HIFI_TTS["url"], shard, HIFI_TTS["size"], HIFI_TTS["sha256"])

    import soundfile
    from pyarrow import parquet

    wanted = arguments.minutes * 60
    taken = 0.0
    folder.mkdir(parents=True, exist_ok=True)
    rows = parquet.ParquetFile(shard)
    for batch in rows.iter_batches(
        columns=["speaker", "file", "duration", "audio"], batch_size=64
    ):
        for row in batch.to_pylist():
            if row["speaker"] != HIFI_TTS["speaker"] or taken >= wanted:
                continue
            audio, rate = soundfile.read(
                io.BytesIO(row["audio"]["bytes"]), dtype="float32"
            )
            stem = Path(row["file"]).stem
            soundfile.write(folder / f"{stem}.wav", audio, rate, subtype="PCM_16")
            taken += row["duration"]
        if taken >= wanted:
            break
    write_credit(arguments.name, HIFI_TTS_CREDIT)
    done.write_text(f"{taken:.0f} seconds\n", encoding="utf-8")
    say(f"{taken / 60:.0f} minutes of speech in {folder}")


def write_credit(name: str, credit: str) -> None:
    (VOICES / name).mkdir(parents=True, exist_ok=True)
    (VOICES / name / "credit.txt").write_text(credit + "\n", encoding="utf-8")


# --------------------------------------------------------------------------------------- train


def experiment(name: str) -> Path:
    return RVC / "logs" / name


def train(arguments: argparse.Namespace) -> None:
    name = arguments.name
    source = dataset_folder(name)
    if not source.exists() or not any(source.iterdir()):
        fail(f"no recordings in {source}; run the dataset step first")
    using_rvc()
    folder = experiment(name)
    folder.mkdir(parents=True, exist_ok=True)
    workers = str(max(1, min(os.cpu_count() or 1, 8)))
    on_gpu = cuda()
    if not on_gpu:
        print(
            "\n    Training on the processor. It works, but it is slow: about 7 seconds a step on"
            "\n    four cores, so 100 epochs of 40 minutes of speech take more than a day. A graphics"
            "\n    card does it in about an hour.",
            flush=True,
        )

    # Written once the pieces are all there, so a stop halfway cuts them again.
    cut = folder / "preprocess.done"
    if not cut.exists():
        event(step="cut")
        say("Cutting the recordings into pieces")
        run(
            [
                "train/preprocess.py",
                source,
                SAMPLE_RATE,
                workers,
                folder,
                False,
                SLICE_SECONDS,
            ],
            folder / "preprocess.log",
        )
        cut.write_text("done\n", encoding="utf-8")

    event(step="pitch")
    say("Finding the pitch of each piece")
    if on_gpu:
        run(
            ["train/dataset/extract_f0.py", "cuda", 1, 0, 0, folder, True],
            folder / "extract_f0_feature.log",
        )
    else:
        run(
            ["train/dataset/extract_f0.py", "cpu", folder, workers, "rmvpe"],
            folder / "extract_f0_feature.log",
        )

    event(step="words")
    say("Finding what is said in each piece")
    run(
        [
            "train/dataset/extract_hubert_feature.py",
            "cuda:0" if on_gpu else "cpu",
            1,
            0,
            folder,
            "v2",
            on_gpu,
        ],
        folder / "extract_f0_feature.log",
    )

    write_file_list(folder)
    config = folder / "config.json"
    if not config.exists():
        shutil.copy(
            RVC / "configs" / "v1" / "40k.json", config
        )  # RVC's own choice for v2-40k

    say(f"Learning the voice: {arguments.epochs} epochs")
    event(
        step="learn",
        epoch=saved_epoch(folder),
        epochs=arguments.epochs,
        device="cuda" if on_gpu else "cpu",
    )
    # Where RVC saves the voice at the end, which its web interface makes when it starts.
    (RVC / "assets" / "weights").mkdir(parents=True, exist_ok=True)
    command = [
        "train/train.py",
        "-e",
        name,
        "-sr",
        "40k",
        "-f0",
        1,
        "-bs",
        arguments.batch,
        "-te",
        arguments.epochs,
        "-se",
        arguments.save_every,
        "-pg",
        "assets/pretrained_v2/f0G40k.pth",
        "-pd",
        "assets/pretrained_v2/f0D40k.pth",
        "-l",
        1,
        "-c",
        0,
        "-sw",
        0,
        "-v",
        "v2",
    ]
    if on_gpu:
        command += ["-g", 0]
    started = [time.monotonic()]

    def on_line(line: str) -> None:
        epoch = EPOCH_LINE.search(line)
        if epoch:
            now = time.monotonic()
            event(
                epoch=int(epoch.group(1)),
                epochs=arguments.epochs,
                seconds=round(now - started[0], 1),
            )
            started[0] = now

    run(command, folder / "train.log", on_line)
    weights = RVC / "assets" / "weights" / f"{name}.pth"
    if not weights.exists():
        fail(f"training ended without {weights}; see {folder / 'train.log'}")
    (VOICES / name).mkdir(parents=True, exist_ok=True)
    trained = {
        "epochs": arguments.epochs,
        "batch": arguments.batch,
        "seconds": round(
            sum(sf_duration(p) for p in (folder / "0_gt_wavs").glob("*.wav"))
        ),
    }
    (VOICES / name / "trained.json").write_text(json.dumps(trained), encoding="utf-8")
    say(f"The voice is learned: {weights}")


def saved_epoch(folder: Path) -> int:
    """The epoch of RVC's last save, which a training that stopped goes on from; 0 for none."""
    checkpoint = folder / "G_2333333.pth"
    if not checkpoint.exists():
        return 0
    import torch

    try:
        saved = torch.load(checkpoint, map_location="cpu", weights_only=False)
    except Exception:  # a save cut short by a stop, which RVC starts over from
        return 0
    return int(saved.get("iteration", 0))


def sf_duration(path: Path) -> float:
    import soundfile

    return soundfile.info(str(path)).duration


def write_file_list(folder: Path) -> None:
    """What RVC's web interface writes before training: each piece with its features and pitch,
    and two pieces of silence."""
    names = None
    for part, suffix in (
        ("0_gt_wavs", ".wav"),
        ("3_feature768", ".npy"),
        ("2a_f0", ".wav.npy"),
        ("2b-f0nsf", ".wav.npy"),
    ):
        found = {p.name[: -len(suffix)] for p in (folder / part).glob("*" + suffix)}
        names = found if names is None else names & found
    if not names:
        fail(f"nothing to learn from in {folder}; see the logs there")
    mute = RVC / "logs" / "mute"
    lines = [
        f"{folder}/0_gt_wavs/{n}.wav|{folder}/3_feature768/{n}.npy|"
        f"{folder}/2a_f0/{n}.wav.npy|{folder}/2b-f0nsf/{n}.wav.npy|0"
        for n in sorted(names)
    ]
    lines += [
        (
            f"{mute}/0_gt_wavs/mute40k.wav|{mute}/3_feature768/mute.npy|"
            f"{mute}/2a_f0/mute.wav.npy|{mute}/2b-f0nsf/mute.wav.npy|0"
        )
    ] * 2
    random.Random(0).shuffle(lines)
    (folder / "filelist.txt").write_text("\n".join(lines), encoding="utf-8")


# -------------------------------------------------------------------------------------- export


def patch_for_onnx() -> None:
    """RVC's attention turns the length of the sequence into Python numbers, which the ONNX
    exporter keeps as constants: the model would only work for the length it was exported with.
    These do the same with tensor operations. They hold for more than 11 frames, which a chunk
    of the app always has."""
    import torch.nn.functional as F
    from infer.module import attentions

    def relative_embeddings(self, embeddings, length):
        pad = length - (self.window_size + 1)
        return F.pad(embeddings, [0, 0, pad, pad, 0, 0])[:, : 2 * length - 1]

    def relative_to_absolute(self, x):
        batch, heads, length, _ = x.size()
        x = F.pad(x, [0, 1, 0, 0, 0, 0, 0, 0])
        flat = F.pad(
            x.reshape([batch, heads, length * 2 * length]), [0, length - 1, 0, 0, 0, 0]
        )
        return flat.reshape([batch, heads, length + 1, 2 * length - 1])[
            :, :, :length, length - 1 :
        ]

    def absolute_to_relative(self, x):
        batch, heads, length, _ = x.size()
        x = F.pad(x, [0, length - 1, 0, 0, 0, 0, 0, 0])
        flat = x.reshape([batch, heads, length * length + length * (length - 1)])
        flat = F.pad(flat, [length, 0, 0, 0, 0, 0])
        return flat.reshape([batch, heads, length, 2 * length])[:, :, :, 1:]

    attention = attentions.MultiHeadAttention
    attention._get_relative_embeddings = relative_embeddings
    attention._relative_position_to_absolute_position = relative_to_absolute
    attention._absolute_position_to_relative_position = absolute_to_relative


@contextlib.contextmanager
def without_randomness():
    """The voice adds a random phase and noise to its sound. Taking them out makes the ONNX
    model and PyTorch give the same numbers, to check one against the other."""
    import torch

    rand, randn_like = torch.rand, torch.randn_like

    def no_rand(*size, **options):
        # The inputs of the check are random on purpose, from a generator of their own.
        if "generator" in options:
            return rand(*size, **options)
        return torch.zeros(*size, **options)

    torch.rand = no_rand
    torch.randn_like = lambda tensor, **options: torch.zeros_like(tensor, **options)
    try:
        yield
    finally:
        torch.rand, torch.randn_like = rand, randn_like


def load_synthesizer(name: str):
    import torch
    from infer.module.models import SynthesizerTrnMs768NSFsid

    weights = RVC / "assets" / "weights" / f"{name}.pth"
    if not weights.exists():
        fail(f"{weights} is missing; run the train step first")
    saved = torch.load(weights, map_location="cpu", weights_only=True)
    if (
        saved.get("version") != "v2"
        or str(saved.get("f0")) != "1"
        or saved.get("sr") != "40k"
    ):
        fail(f"{weights} is not an RVC v2 voice of 40 kHz with pitch")
    config = list(saved["config"])
    config[-3] = saved["weight"]["emb_g.weight"].shape[
        0
    ]  # speakers, as RVC itself does
    net = SynthesizerTrnMs768NSFsid(*config, is_half=False)
    del net.enc_q  # only used to train
    net.load_state_dict(
        {k: v.float() for k, v in saved["weight"].items()}, strict=False
    )
    net.eval()
    net.remove_weight_norm()
    return net


def synthesizer_module(net):
    import torch

    class Synthesizer(torch.nn.Module):
        """The voice for one chunk of the stream, as RVC's realtime converter runs it: the
        encoder reads the whole window, the flow its last frames, and the decoder only the
        frames the app plays. How many of each comes from the length of `noise` and
        `pitchf`."""

        def __init__(self):
            super().__init__()
            self.net = net

        def forward(self, phone, phone_lengths, pitch, pitchf, sid, noise):
            n = self.net
            g = n.emb_g(sid).unsqueeze(-1)
            m_p, logs_p, x_mask = n.enc_p(phone, pitch, phone_lengths)
            flow, played = noise.shape[2], pitchf.shape[1]
            m_p, logs_p, x_mask = (
                m_p[:, :, -flow:],
                logs_p[:, :, -flow:],
                x_mask[:, :, -flow:],
            )
            z = n.flow(
                (m_p + torch.exp(logs_p) * noise) * x_mask, x_mask, g=g, reverse=True
            )
            return n.dec((z * x_mask)[:, :, -played:], pitchf, g=g)

    return Synthesizer().eval()


def content_module():
    import torch
    from infer.hubert import HUBERT_MODEL_PATH, HubertModelWithFinalProj

    model = HubertModelWithFinalProj.from_pretrained(str(HUBERT_MODEL_PATH)).eval()

    class Content(torch.nn.Module):
        """What is said, without who says it: the last layer of HuBERT, as RVC v2 uses it,
        50 frames a second of 16 kHz sound."""

        def __init__(self):
            super().__init__()
            self.model = model

        def forward(self, audio):
            return self.model(input_values=audio).last_hidden_state

    return Content().eval()


def synthesizer_inputs(frames: int, played: int, seed: int = 0):
    import torch

    generator = torch.Generator().manual_seed(seed)
    hz = 150 + 80 * torch.rand(1, frames, generator=generator)
    return (
        torch.randn(1, frames, 768, generator=generator),
        torch.tensor([frames], dtype=torch.long),
        coarse_pitch(hz),
        hz[:, -played:],
        torch.tensor([0], dtype=torch.long),
        torch.randn(1, 192, played + FLOW_CONTEXT, generator=generator) * 0.66666,
    )


def coarse_pitch(hz):
    """The 255 steps of pitch the encoder of RVC reads, from 50 to 1100 Hz on the mel scale;
    1 is no pitch."""
    import torch

    low, high = 1127 * math.log(1 + 50 / 700), 1127 * math.log(1 + 1100 / 700)
    mel = 1127 * torch.log(1 + hz / 700)
    steps = torch.where(mel > 0, (mel - low) * 254 / (high - low) + 1, mel)
    return steps.clamp(1, 255).round().long()


SYNTHESIZER_IO = {
    "input_names": ["phone", "phone_lengths", "pitch", "pitchf", "sid", "noise"],
    "output_names": ["audio"],
    "dynamic_axes": {
        "phone": {1: "frames"},
        "pitch": {1: "frames"},
        "pitchf": {1: "played"},
        "noise": {2: "flow"},
        "audio": {2: "samples"},
    },
}


def export_onnx(module, inputs, path: Path, **io_names) -> None:
    import torch

    torch.onnx.export(
        module,
        inputs,
        str(path),
        opset_version=17,
        dynamo=False,
        do_constant_folding=False,
        **io_names,
    )


def session(path: Path):
    import onnxruntime

    return onnxruntime.InferenceSession(str(path), providers=["CPUExecutionProvider"])


def export(arguments: argparse.Namespace) -> None:
    import numpy as np
    import torch

    name = arguments.name
    event(step="export")
    using_rvc()
    patch_for_onnx()
    out = VOICES / name / "app"
    out.mkdir(parents=True, exist_ok=True)
    net = load_synthesizer(name)

    say("Exporting the voice to ONNX")
    synthesizer = out / "synthesizer.onnx"
    with torch.no_grad():
        export_onnx(
            synthesizer_module(net),
            synthesizer_inputs(200, 60),
            synthesizer,
            **SYNTHESIZER_IO,
        )

    say("Checking it against PyTorch, at lengths it was not exported with")
    checked = out / "synthesizer.check.onnx"
    with torch.no_grad(), without_randomness():
        module = synthesizer_module(net)
        export_onnx(module, synthesizer_inputs(200, 60), checked, **SYNTHESIZER_IO)
        exported = session(checked)
        for frames, played in ((90, 30), (321, 100)):
            inputs = synthesizer_inputs(frames, played, seed=frames)
            expected = module(*inputs).numpy()
            got = exported.run(
                None,
                dict(zip(SYNTHESIZER_IO["input_names"], (x.numpy() for x in inputs))),
            )[0]
            if got.shape != expected.shape or got.shape[2] != played * HOP:
                fail(f"the ONNX voice gives {got.shape} samples for {played} frames")
            difference = float(np.abs(got - expected).max())
            print(
                f"    {frames} frames, {played} played: largest difference {difference:.1e}"
            )
            if difference > 1e-3:
                fail("the ONNX voice does not sound like the PyTorch one")
    checked.unlink()
    with torch.no_grad():
        sound = session(synthesizer).run(
            None,
            dict(
                zip(
                    SYNTHESIZER_IO["input_names"],
                    (x.numpy() for x in synthesizer_inputs(150, 50)),
                )
            ),
        )[0]
    if not np.isfinite(sound).all():
        fail("the ONNX voice makes no sound")

    content = out / "content.onnx"
    if not content.exists():
        say("Exporting what the voice listens with (HuBERT, once for every voice)")
        module = content_module()
        # Written aside and moved in once checked, so a stop never leaves half of it.
        part = out / "content.part.onnx"
        with torch.no_grad():
            export_onnx(
                module,
                (torch.randn(1, CONTENT_RATE) * 0.1,),
                part,
                input_names=["audio"],
                output_names=["features"],
                dynamic_axes={"audio": {1: "samples"}, "features": {1: "frames"}},
            )
            audio = torch.randn(1, 23_456) * 0.1
            difference = float(
                np.abs(
                    session(part).run(None, {"audio": audio.numpy()})[0]
                    - module(audio).numpy()
                ).max()
            )
        print(f"    largest difference from PyTorch: {difference:.1e}")
        if difference > 1e-3:
            part.unlink()
            fail("the ONNX HuBERT does not hear like the PyTorch one")
        part.replace(content)

    trained = VOICES / name / "trained.json"
    voice = {
        "format": FORMAT,
        "engine": "rvc",
        "name": arguments.title or name,
        "sample_rate": SAMPLE_RATE,
        "frame_rate": SAMPLE_RATE // HOP,
        "pitch": {"median_hz": round(median_pitch(experiment(name)), 1)},
        "content": {
            "file": content.name,
            "sha256": sha256(content),
            "size": content.stat().st_size,
            "sample_rate": CONTENT_RATE,
            "frame_rate": CONTENT_RATE // 320,
        },
        "synthesizer": {
            "file": synthesizer.name,
            "sha256": sha256(synthesizer),
            "size": synthesizer.stat().st_size,
            "speaker": 0,
            "flow_context": FLOW_CONTEXT,
        },
        "credit": (VOICES / name / "credit.txt").read_text(encoding="utf-8").strip()
        if (VOICES / name / "credit.txt").exists()
        else "",
        "trained": json.loads(trained.read_text()) if trained.exists() else {},
        "rvc": RVC_COMMIT,
    }
    (out / "voice.json").write_text(
        json.dumps(voice, indent=2) + "\n", encoding="utf-8"
    )
    event(exported=str(out))
    say(f"The voice for the app is in {out}")
    for file in sorted(out.iterdir()):
        print(f"    {file.name}: {file.stat().st_size / 1e6:.1f} MB")


def median_pitch(folder: Path) -> float:
    """The pitch the voice usually speaks at, which the app moves the user's voice to."""
    import numpy as np

    pitches = [np.load(p) for p in (folder / "2b-f0nsf").glob("*.wav.npy")]
    voiced = np.concatenate([p[p > 0] for p in pitches]) if pitches else np.array([])
    if voiced.size == 0:
        fail(f"no pitch found in {folder / '2b-f0nsf'}")
    return float(np.median(voiced))


# ----------------------------------------------------------------------------------------- try


def try_voice(arguments: argparse.Namespace) -> None:
    """Converts a whole recording with the exported files, the way the app will: HuBERT reads
    it, its pitch moves to the voice's, and the synthesizer speaks it."""
    import librosa
    import numpy as np
    import soundfile
    import torch

    recording = Path(arguments.recording).expanduser().resolve()
    using_rvc()
    folder = VOICES / arguments.name / "app"
    voice = json.loads((folder / "voice.json").read_text())
    audio, _ = librosa.load(str(recording), sr=CONTENT_RATE, mono=True)

    from infer.rmvpe import RMVPE

    rmvpe = RMVPE("assets/rmvpe/rmvpe.pt", is_half=False, device="cpu")
    hz = rmvpe.infer_from_audio(audio, thred=0.03)  # 100 frames a second
    voiced = hz[hz > 0]
    if voiced.size == 0:
        fail(f"no voice heard in {recording}")
    shift = (
        arguments.shift
        if arguments.shift is not None
        else 12 * math.log2(voice["pitch"]["median_hz"] / float(np.median(voiced)))
    )
    hz = hz * 2 ** (shift / 12)
    print(f"    pitch moved by {shift:+.1f} semitones")

    features = session(folder / voice["content"]["file"]).run(
        None, {"audio": audio[None, :].astype(np.float32)}
    )[0]
    features = np.repeat(features, 2, axis=1)  # 50 to 100 frames a second, as RVC does
    frames = min(features.shape[1], hz.shape[0])
    features, hz = features[:, :frames], hz[None, :frames].astype(np.float32)
    rng = np.random.default_rng(0)
    sound = session(folder / voice["synthesizer"]["file"]).run(
        None,
        {
            "phone": features.astype(np.float32),
            "phone_lengths": np.array([frames], dtype=np.int64),
            "pitch": coarse_pitch(torch.from_numpy(hz)).numpy(),
            "pitchf": hz,
            "sid": np.array([voice["synthesizer"]["speaker"]], dtype=np.int64),
            "noise": (rng.standard_normal((1, 192, frames)) * 0.66666).astype(
                np.float32
            ),
        },
    )[0][0, 0]
    target = recording.with_name(f"{recording.stem}.{arguments.name}.wav")
    soundfile.write(target, sound, voice["sample_rate"])
    say(f"Listen to {target}")


# ----------------------------------------------------------------------------------------- cli


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--events", action="store_true", help="write the progress as JSON for the app"
    )
    parser.add_argument(
        "--end-with-input",
        action="store_true",
        help="for the app, on Linux: end this step and what it started when the input closes",
    )
    steps = parser.add_subparsers(dest="step", required=True)
    steps.add_parser("setup", help="downloads RVC and its base models")

    def named(step: str, text: str) -> argparse.ArgumentParser:
        command = steps.add_parser(step, help=text)
        command.add_argument("--name", required=True, help="what the voice is called")
        return command

    def dataset_options(command: argparse.ArgumentParser) -> None:
        command.add_argument(
            "--minutes",
            type=int,
            default=40,
            help="how much of Hi-Fi TTS to learn from (default 40)",
        )
        command.add_argument(
            "--folder", help="learn from your own recordings in this folder instead"
        )
        command.add_argument("--credit", help="who the recordings of --folder are from")

    def train_options(command: argparse.ArgumentParser) -> None:
        command.add_argument(
            "--epochs",
            type=int,
            default=200,
            help="times the training goes through the recordings (default 200)",
        )
        command.add_argument(
            "--batch",
            type=int,
            default=8,
            help="pieces learned at once; lower it if the card runs out of memory (default 8)",
        )
        command.add_argument(
            "--save-every",
            type=int,
            default=25,
            help="epochs between saves, to resume after a stop (default 25)",
        )

    dataset_options(named("dataset", "takes the recordings the voice learns from"))
    train_options(named("train", "learns the voice"))
    def export_options(command: argparse.ArgumentParser) -> None:
        command.add_argument(
            "--title", help="the name the app shows for the voice; by default --name"
        )

    export_options(named("export", "writes the voice for the app, and checks it"))
    trying = named("try", "converts a recording to the voice, to hear it")
    trying.add_argument("recording", help="a recording of someone talking")
    trying.add_argument(
        "--shift",
        type=float,
        help="semitones to move the pitch; by default, to the voice's own",
    )
    everything = named("all", "dataset, train and export")
    dataset_options(everything)
    train_options(everything)
    export_options(everything)

    arguments = parser.parse_args()
    global EVENTS
    EVENTS = arguments.events
    if arguments.end_with_input:
        end_with_input()
    if arguments.step == "setup":
        setup(arguments)
    elif arguments.step == "dataset":
        dataset(arguments)
    elif arguments.step == "train":
        train(arguments)
    elif arguments.step == "export":
        export(arguments)
    elif arguments.step == "try":
        try_voice(arguments)
    else:
        dataset(arguments)
        train(arguments)
        export(arguments)


if __name__ == "__main__":
    main()
