; Per-user installer: no admin prompt, and the app can replace itself when it updates.
; Build with: iscc /DAppVersion=1.2.3 /DPublishDir=..\publish build\installer.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef PublishDir
  #define PublishDir "..\publish"
#endif

[Setup]
AppId={{7E2B6C1F-3D44-4E0B-9A57-1C5F0D2A9B61}
AppName=InstantPad
AppVersion={#AppVersion}
AppPublisher=eljonathas
AppPublisherURL=https://github.com/eljonathas/InstantPad
DefaultDirName={autopf}\InstantPad
DefaultGroupName=InstantPad
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#PublishDir}
OutputBaseFilename=InstantPadSetup
SetupIconFile=..\assets\InstantPad.ico
UninstallDisplayIcon={app}\InstantPad.exe
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
; Matches SingleInstanceGuard, so the installer asks to close a running copy.
AppMutex=InstantPad.SingleInstance

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#PublishDir}\InstantPad.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\InstantPad"; Filename: "{app}\InstantPad.exe"
Name: "{autodesktop}\InstantPad"; Filename: "{app}\InstantPad.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\InstantPad.exe"; Description: "{cm:LaunchProgram,InstantPad}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\InstantPad.exe.old"
Type: files; Name: "{app}\InstantPad.exe.new"
