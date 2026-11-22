; Windows installer for Areadox Blur (Inno Setup 6).
; Installs into the OBS Studio 33+ plugin location:
;   C:\ProgramData\obs-studio\plugins\areadox-blur\areadox-blur.dll
;   C:\ProgramData\obs-studio\plugins\areadox-blur\data\...
;
; Build:  iscc /DAppVersion=0.1.0 /DSourceDir=..\dist\areadox-blur installer\areadox-blur.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\dist\areadox-blur"
#endif

[Setup]
AppId={{0B6F17B0-3321-4ADE-A81D-1AE12DD80442}
AppName=Areadox Blur (OBS-Plugin)
AppVersion={#AppVersion}
AppPublisher=areadox
AppPublisherURL=https://areadox.de
AppSupportURL=https://github.com/areadox/areadox-blur/issues
DefaultDirName={commonappdata}\obs-studio\plugins\areadox-blur
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=areadox-blur-{#AppVersion}-windows-x64-installer
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile=..\LICENSE
UninstallDisplayName=Areadox Blur (OBS-Plugin)
CloseApplications=yes

[Languages]
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#SourceDir}\areadox-blur.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\data\*"; DestDir: "{app}\data"; Flags: ignoreversion recursesubdirs createallsubdirs

[UninstallDelete]
Type: dirifempty; Name: "{app}"

[Code]
function InitializeSetup(): Boolean;
begin
  Result := True;
  if CheckForMutexes('OBSStudio') then
  begin
    if MsgBox(CustomMessage('ObsRunning'), mbConfirmation, MB_OKCANCEL) = IDCANCEL then
      Result := False;
  end;
end;

[CustomMessages]
de.ObsRunning=OBS Studio scheint zu laufen. Bitte OBS schließen, damit das Plugin beim nächsten Start geladen wird.%n%nTrotzdem fortfahren?
en.ObsRunning=OBS Studio seems to be running. Please close OBS so the plugin is loaded on the next start.%n%nContinue anyway?
