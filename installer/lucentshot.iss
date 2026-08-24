#define MyAppName "LucentShot"
#ifndef MyAppVersion
#error "Pass /DMyAppVersion=x.y.z from Cargo.toml"
#endif
#define MyAppPublisher "Dvow"
#define MyAppExeName "lucentshot.exe"

[Setup]
AppId={{B7E2C4A1-9F18-4D6B-8C3A-1E5F0A2B7D44}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
LicenseFile=LICENSE
DisableProgramGroupPage=yes
OutputDir=dist
OutputBaseFilename=LucentShot-{#MyAppVersion}-setup
SetupIconFile=assets\icons\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
SourceDir=..
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\tesseract.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\leptonica-1.85.0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\eng.traineddata"; DestDir: "{app}\tessdata"; Flags: ignoreversion
Source: "LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
