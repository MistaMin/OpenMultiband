; Windows installer for OpenMultiband: VST3, CLAP and LV2, one choice per format.
; Built by .github/workflows/build.yml:  iscc /DVersion=1.0.0-Beta /DDist=<dist> /DRoot=<repo> installer\windows.iss
#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Dist
  #define Dist "..\dist-windows"
#endif
#ifndef Root
  #define Root ".."
#endif

[Setup]
AppId={{A8B1DE41-8D42-4274-81FF-8AE2C50A9AAE}
AppName=OpenMultiband
AppVersion={#Version}
AppPublisher=OpenGrid
AppPublisherURL=https://opengrid.studio
DefaultDirName={commonpf64}\OpenGrid\OpenMultiband
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
PrivilegesRequired=admin
LicenseFile={#Root}\BINARY_LICENSE.txt
OutputBaseFilename=OpenMultiband-{#Version}-windows-x64-setup
OutputDir={#Dist}\..
Compression=lzma2
SolidCompression=yes
UninstallDisplayName=OpenMultiband {#Version}

[Types]
Name: "full"; Description: "VST3, CLAP and LV2"
Name: "custom"; Description: "Choose formats"; Flags: iscustom

[Components]
Name: "vst3"; Description: "VST3 plug-in"; Types: full
Name: "clap"; Description: "CLAP plug-in"; Types: full
Name: "lv2"; Description: "LV2 plug-in"; Types: full

[Files]
Source: "{#Dist}\OpenMultiband.vst3\*"; DestDir: "{commoncf64}\VST3\OpenMultiband.vst3"; Components: vst3; Flags: recursesubdirs ignoreversion
Source: "{#Dist}\OpenMultiband.clap"; DestDir: "{commoncf64}\CLAP"; Components: clap; Flags: ignoreversion
Source: "{#Dist}\OpenMultiband.lv2\*"; DestDir: "{commoncf64}\LV2\OpenMultiband.lv2"; Components: lv2; Flags: recursesubdirs ignoreversion
Source: "{#Root}\LICENSE"; DestDir: "{app}\Licenses"; DestName: "LICENSE.txt"
Source: "{#Root}\BINARY_LICENSE.txt"; DestDir: "{app}\Licenses"
Source: "{#Root}\THIRD_PARTY_LICENSES.md"; DestDir: "{app}\Licenses"
Source: "{#Root}\Licenses\*"; DestDir: "{app}\Licenses"; Flags: recursesubdirs
