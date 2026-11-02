# 0003: AI voices run locally, on models downloaded on demand

The AI voices change the timbre of the user's voice, not only its pitch. They had to keep three promises of the app: a free download that stays small, no account or server, and nothing the user says leaving the computer.

So the models run on the user's processor with ONNX Runtime, and nothing is uploaded. The installer does not grow: the runtime library and the models, about 260 MB, download the first time an AI voice is used, from pinned URLs, and each file is checked against a size and a SHA-256 kept in the code.

- **Conversion** uses the tone color converter of OpenVoice V2 (MIT), from an ONNX export pinned to one revision. A measurement on a 4-core 2.1 GHz Xeon converted 139 ms of voice in about 89 ms, in chunks with 197 ms of past audio and 35 ms ahead, for about 275 ms of delay. Smaller chunks lose quality and larger ones add delay.
- **The user's own voice** must be known to take it out of the voice. A voiceprint averaged over a few seconds of reading was clearly better than one estimated from each chunk, so the user reads once before the first AI voice.
- **Feminine AI** speaks with the average voiceprint of ten LibriSpeech speakers, so it imitates no real person. The converter keeps the melody of the voice, so the effect raises the pitch first.
- **ONNX Runtime 1.23.2** is the last version with an Intel macOS build, and the app ships one. The `ort` crate loads the library at run time and asks it only for an old API version. The library comes from the official PyPI wheel, which is a zip on every system.
- **The audio callback never waits for the AI.** It hands the voice to a worker thread through a ring and plays what comes back once a chunk is buffered. When the worker falls behind, the app warns and keeps only the pitch part of the effect, so a slow computer never stutters in a match.

GPU acceleration is left out: the game already uses the GPU, and a voice that lowers the frame rate is worse than one that arrives a little later.
