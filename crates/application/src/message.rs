use serde::Serialize;
use specta::Type;
use thiserror::Error;

/// A key of the translated texts. The frontend owns the translations, so TypeScript refuses
/// a variant that is missing from them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
pub enum TextKey {
    AudioDeviceNotResponding,
    CatalogNoConnection,
    DevelopmentVersion,
    DeviceUnavailable,
    Downloading,
    DownloadingVersion,
    DriverDownloading,
    DriverFailed,
    DriverInstallCanceled,
    DriverOpeningPage,
    DriverOpeningSetup,
    DriverSearching,
    DriverSetupMissing,
    DriverSignatureInvalid,
    DriverSignatureUnreadable,
    DriverSignerUnexpected,
    DriverVerifying,
    EffectCycle,
    EffectDeep,
    EffectNatural,
    EffectThin,
    EmptyPad,
    FailedSound,
    GameMicrophoneHint,
    HotkeyInUse,
    InGameSound,
    InterfaceFull,
    InterfaceFullHint,
    Listening,
    MicStopped,
    Missing,
    NoDriverHint,
    Ok,
    OutputStopped,
    PadSelected,
    ParamFormant,
    ParamHigh,
    ParamIntensity,
    ParamLow,
    ParamTime,
    RoutingStatus,
    SaveFailed,
    Saved,
    SentTo,
    SoundCopyFailed,
    SoundDownloadFailed,
    SoundFileMissing,
    SoundHasNoAudio,
    SoundOpenFailed,
    SoundSaveFailed,
    Stopped,
    SystemOutputOnCableHint,
    UpdateDownloadFailed,
    Version,
    VersionInstalled,
}

/// Text for the user. The frontend renders it, so it follows a language change.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Message {
    Text { key: TextKey, args: Vec<Message> },
    Plain { text: String },
}

impl Message {
    pub fn text(key: TextKey) -> Self {
        Self::with(key, [])
    }

    pub fn with<const N: usize>(key: TextKey, args: [Message; N]) -> Self {
        Self::Text {
            key,
            args: args.into(),
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::Plain { text: text.into() }
    }
}

/// Something a port could not do, in words for the user.
#[derive(Clone, Debug, Error, PartialEq, Serialize, Type)]
#[error("{0:?}")]
pub struct Failure(pub Message);

impl Failure {
    pub fn text(key: TextKey) -> Self {
        Self(Message::text(key))
    }

    /// The translated `key` around a detail from the system, such as an OS error.
    pub fn because(key: TextKey, detail: impl ToString) -> Self {
        Self(Message::with(key, [Message::plain(detail.to_string())]))
    }
}
