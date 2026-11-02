# 0004: A voice is only cloned with its owner present

A voice changer that speaks as someone else, live in calls, can be used to deceive people. Cloning had to work for a friend who agrees, and fail for a voice taken from a video or a voice message.

So cloning asks for two proofs, like the professional voice cloning of ElevenLabs:

- **A declaration.** The owner accepts each clause of the terms and writes their name. The terms are versioned: raising `TERMS_VERSION` deletes every voice cloned under older terms the next time the app opens, since their owners never agreed to the new ones.
- **A live check.** The owner reads six random digits. Speech recognition must find the digits, forgiving one, and the voiceprint of the reading must match the sample. A recording of the digits cannot exist beforehand, so whoever reads them is at the microphone. On LibriSpeech the voiceprint threshold of 0.80 rejected 2.5% of true matches and accepted 1.4% of different people; the digits carry the rest of the check.

A voice is biometric data under the LGPD, so the app keeps as little as it can: the voiceprint and the consent record (owner, date, terms version, similarity and the SHA-256 of the sample). Recordings live only in memory while cloning and are never written to disk. Deleting a voice deletes its consent record too.

A voice designed from cloned ones, by mixing them or moving their timbre, keeps only its recipe. Its voiceprint is worked out from the cloned voices each time, so deleting a cloned voice deletes every voice designed from it and nothing of the owner's voice is left behind.

The rules live in `crates/domain/src/cloning.rs`, so the interface cannot skip a step.
