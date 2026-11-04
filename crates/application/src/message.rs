use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

/// A key of the translated texts. The frontend owns the translations, so TypeScript refuses
/// a variant that is missing from them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum TextKey {
    AiCalibrationSaved,
    AiDownloadFailed,
    AiInstalled,
    AiListening,
    AiLoadFailed,
    AiNotCalibrated,
    AiNotInstalled,
    AiTooSlow,
    AiTrainedTooSlow,
    AudioDeviceNotResponding,
    CatalogNoConnection,
    CloneDeleted,
    CloneMissingName,
    CloneSaved,
    CloneVoice,
    CutsPlaced,
    DevelopmentVersion,
    DeviceBack,
    DeviceGaveUp,
    DeviceLost,
    DeviceStandIn,
    DeviceStopped,
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
    EffectFeminine,
    EffectFeminineAi,
    EffectGhost,
    EffectNatural,
    EffectOgre,
    EffectThin,
    EmptyPad,
    FailedSound,
    GameMicrophoneHint,
    HotkeyInUse,
    InGameSound,
    InterfaceFull,
    InterfaceFullHint,
    Listening,
    MicrophoneBusy,
    Missing,
    NoDriverHint,
    Ok,
    PadSelected,
    ParamEcho,
    ParamFormant,
    ParamGrowl,
    ParamHigh,
    ParamIntensity,
    ParamLow,
    ParamPitch,
    ParamTime,
    ParamWhisper,
    RoleGameOutput,
    RoleMicrophone,
    RoleMonitorOutput,
    RoutingStatus,
    SampleTooLong,
    SampleTooShort,
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
    TakeTooShort,
    TrainedModelAdded,
    TrainedModelBusy,
    TrainedModelInvalid,
    TrainedModelRemoved,
    TrainingAlreadyRunning,
    TrainingDone,
    TrainingFailed,
    TrainingFull,
    TrainingInstallFailed,
    TrainingPaused,
    TrainingToolsRemoved,
    TrainingUnsupported,
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

    /// A limit the user went past, with the number of seconds it allows.
    pub(crate) fn limit(key: TextKey, seconds: u32) -> Self {
        Self::with(key, [Self::plain(seconds.to_string())])
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
