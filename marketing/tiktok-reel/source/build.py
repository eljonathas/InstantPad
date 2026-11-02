import json, subprocess, sys, os, bisect
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

S = os.path.dirname(os.path.abspath(__file__))
R, CAP, CARDS = f"{S}/reel", f"{S}/cap", f"{S}/cards"
OUT = f"{S}/out"; os.makedirs(OUT, exist_ok=True)
SR, FPS, W, H = 48000, 30, 1080, 1920
TOTAL = 41.0
MUSIC = sys.argv[1] if len(sys.argv) > 1 else "music1.mp3"

def load(path, ch=1):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-ac", str(ch), "-ar", str(SR), "-f", "f32le", "-"], capture_output=True, check=True).stdout
    a = np.frombuffer(raw, np.float32)
    return a.reshape(-1, ch) if ch > 1 else a

# ---------- narration with InstantPad's own voice effects ----------
nat = np.fromfile(f"{R}/nat.f32", np.float32)
LAT = int(0.040 * SR)  # the effect's 40 ms window
deep = np.fromfile(f"{R}/deep.f32", np.float32)[LAT:]; deep = np.pad(deep, (0, len(nat) - len(deep)))
thin = np.fromfile(f"{R}/thin.f32", np.float32)[LAT:]; thin = np.pad(thin, (0, len(nat) - len(thin)))

def window(n, a, b, fade=0.015):
    t = np.arange(n) / SR
    return np.clip(np.minimum((t - a) / fade, (b - t) / fade) + 0.5, 0, 1)

wd, wt = window(len(nat), 21.66, 22.78), window(len(nat), 23.22, 24.22)
voice = nat * (1 - wd - wt) + deep * wd + thin * wt

# (narration from, to) -> output start
SEGS = [(0.00, 4.62, 0.00), (4.62, 7.08, 4.62), (7.08, 13.25, 11.00), (13.25, 16.28, 17.17),
        (16.28, 20.74, 21.60), (20.74, 24.19, 26.10), (24.19, 29.63, 29.60), (29.63, 34.10, 35.10)]
def out_time(t):
    for a, b, o in SEGS:
        if a <= t < b: return t - a + o
    for a, b, o in SEGS:
        if a - 0.1 <= t <= b + 0.1: return t - a + o
    raise ValueError(t)

N = int(TOTAL * SR)
vtrack = np.zeros(N, np.float32)
for a, b, o in SEGS:
    seg = voice[int(a * SR):int(b * SR)].copy()
    f = int(0.006 * SR); ramp = np.linspace(0, 1, f)
    seg[:f] *= ramp; seg[-f:] *= ramp[::-1]
    i = int(o * SR); vtrack[i:i + len(seg)] += seg

# ---------- sound effects: the pads being pressed ----------
fx = np.zeros(N, np.float32)
def place(name, at, gain, fade_at=None, fade=0.5):
    s = load(f"{R}/{name}") * gain
    i = int(at * SR); s = s[:N - i].copy()
    if fade_at is not None:
        t = np.arange(len(s)) / SR + at
        s *= np.clip(1 - (t - fade_at) / fade, 0, 1)
    fx[i:i + len(s)] += s
MK = json.load(open(f"{S}/motion/take/frames.json"))["marks"]
place("sfx_boom.mp3", MK["vine"] + 0.02, 0.85)
place("sfx_airhorn.mp3", MK["airhorn"] + 0.02, 0.55)
place("sfx_trombone.mp3", MK["trombone"] + 0.02, 0.75, fade_at=10.95, fade=0.5)
place("sfx_applause.mp3", MK["applause"] + 0.02, 0.55, fade_at=21.55, fade=0.5)
place("sfx_boom.mp3", 39.45, 0.9)

# ---------- music bed, ducked under the voice ----------
mus = load(f"{R}/{MUSIC}")[:N]; mus = np.pad(mus, (0, N - len(mus)))
env = np.abs(vtrack); k = int(0.25 * SR)
env = np.convolve(env, np.ones(k) / k, mode="same")
talking = np.clip(env / 0.02, 0, 1)
t = np.arange(N) / SR
gain = 0.20 - 0.11 * talking
gain = np.where((t > 7.0) & (t < 10.9), 0.12, gain)       # let the pads speak
gain = np.where(t > 39.4, 0.0, gain)
gain *= np.clip(t / 0.4, 0, 1) * np.clip((TOTAL - 0.2 - t) / 1.0, 0, 1)
gain = np.convolve(gain, np.ones(2400) / 2400, mode="same")
mix = vtrack + fx + mus * gain
mix /= max(1.0, np.abs(mix).max() / 0.95)
mix.astype(np.float32).tofile(f"{OUT}/mix.f32")
subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ar", str(SR), "-ac", "1", "-i", f"{OUT}/mix.f32",
                "-af", "loudnorm=I=-14:TP=-1.5:LRA=9", "-ar", "48000", "-ac", "2", f"{OUT}/mix.wav"], check=True)

# ---------- video: the motion take, resampled to 30 fps by timestamp ----------
d = json.load(open(f"{S}/motion/take/frames.json")); T = d["times"]
lst = []
for n in range(int(TOTAL * FPS)):
    i = max(0, bisect.bisect_right(T, n / FPS) - 1)
    lst.append(f"file '{S}/motion/take/{i:05d}.jpg'\nduration {1/FPS:.6f}")
open(f"{OUT}/frames.txt", "w").write("\n".join(lst) + "\n")
subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", f"{OUT}/frames.txt", "-vf", f"fps={FPS},format=yuv420p",
                "-c:v", "libx264", "-preset", "medium", "-crf", "12", f"{OUT}/video_raw.mp4"], check=True)

# ---------- captions ----------
words = json.load(open(f"{R}/words.json"))
CH = [  # (first word index, last word index) chunks, by narration order
]
def ts(x):
    h = int(x // 3600); m = int(x % 3600 // 60); s = x % 60
    return f"{h}:{m:02d}:{s:05.2f}"
HL = {"meme", "InstantPad", "MyInstants", "pronto", "Discord", "jogo", "grossa", "fininha", "graça", "aberto", "Linux", "call", "som", "pad"}
chunks = json.load(open(f"{S}/chunks.json"))
ev = []
for text, a, b in chunks["voice"]:
    oa, ob = out_time(a), out_time(b)
    parts = []
    for w in text.split(" "):
        bare = w.strip(".,:?!…")
        parts.append("{\\c&H3AB0FF&}" + w + "{\\c&HFFFFFF&}" if bare in HL else w)
    ev.append(f"Dialogue: 0,{ts(oa)},{ts(ob)},Cap,,0,0,0,,{{\\fad(40,40)\\t(0,90,\\fscx108\\fscy108)\\t(90,180,\\fscx100\\fscy100)}}{' '.join(parts)}")
for text, a, b in chunks["sfx"]:
    ev.append(f"Dialogue: 0,{ts(a)},{ts(b)},Sfx,,0,0,0,,{{\\fad(30,60)}}{text}")
for text, a, b in chunks["head"]:
    ev.append(f"Dialogue: 1,{ts(a)},{ts(b)},Head,,0,0,0,,{{\\fad(0,120)}}{text}")
ass = f"""[Script Info]
ScriptType: v4.00+
PlayResX: {W}
PlayResY: {H}
WrapStyle: 0

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Cap,Lexend Black,88,&H00FFFFFF,&H00FFFFFF,&H00101010,&H96000000,-1,0,0,0,100,100,0,0,1,7,3,5,80,80,0,1
Style: Sfx,Lexend Black,64,&H00101010,&H00101010,&H003AB0FF,&H003AB0FF,-1,0,0,0,100,100,1,0,3,18,0,5,80,80,0,1
Style: Head,Lexend Black,86,&H00101010,&H00101010,&H003AB0FF,&H003AB0FF,-1,0,0,0,100,100,0,0,3,22,0,5,60,60,0,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
""" + "\n".join(e.replace(",,{", ",,{\\pos(540,1560)" if ",Cap," in e else (",,{\\pos(540,1520)" if ",Sfx," in e else ",,{\\pos(540,300)"), 1) for e in ev) + "\n"
open(f"{OUT}/captions.ass", "w").write(ass)
subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", f"{OUT}/video_raw.mp4", "-i", f"{OUT}/mix.wav",
                "-vf", f"ass={OUT}/captions.ass:fontsdir={CARDS}", "-c:v", "libx264", "-preset", "slow", "-crf", "17",
                "-profile:v", "high", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart",
                "-shortest", f"{OUT}/instantpad_reel_tiktok.mp4"], check=True)
print("done")
