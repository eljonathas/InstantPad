# 0005: Cloning a voice asks for a sample and a name

[ADR 0004](0004-voice-cloning-consent.md) made cloning go through terms to accept, with the owner's name, and a live check of six random digits that had to come from the voice of the sample. Users complained about both, over and over, and asked for them to go. Cloning is now a sample and a name.

- **No terms, no name of the owner, no digits.** Neither the declaration nor the check is asked for, and the app no longer keeps a consent record. Voices saved by earlier versions still load, and their old record is ignored. The terms version is gone too, so no voice is deleted for having been cloned under older terms.
- **The app does not know whose voice it clones.** A recording taken from a video or a voice message clones as well as one made at the microphone, and a voice that speaks live in calls can be used to deceive people. That is what ADR 0004 was written to prevent, and the decision here accepts it: the friction was costing more users than it protected. The README tells the user the app checks nothing, and that passing oneself off as someone is theirs to answer for.
- **The speech recognizer goes.** Its only job was to read back the digits, so the Whisper models no longer download, which takes the download from about 260 MB to about 180 MB, and the app deletes the files of earlier versions when it starts. The hash of the sample, which only the consent record held, goes as well.
- **What stays from ADR 0004.** A voice is biometric data under the LGPD, so the app still keeps as little as it can: the voiceprint, never the recording, which lives only in memory while cloning. Deleting a voice deletes every voice designed from it, since a designed voice keeps only its recipe.

The rules live in `crates/domain/src/cloning.rs`.
