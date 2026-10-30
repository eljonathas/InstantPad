namespace InstantPad.Application.Audio;

/// <summary>A sound file is missing or cannot be decoded.</summary>
public sealed class SoundUnavailableException(string message, Exception? inner = null) : Exception(message, inner);

/// <summary>An audio device refused to open.</summary>
public sealed class AudioDeviceException(string message, Exception? inner = null) : Exception(message, inner);

/// <summary>The driver installation did not finish, for a reason the user should read.</summary>
public sealed class DriverInstallException(string message, Exception? inner = null) : Exception(message, inner);

/// <summary>The user declined the elevation prompt of the driver setup.</summary>
public sealed class DriverInstallCanceledException : Exception;
