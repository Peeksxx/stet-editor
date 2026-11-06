#ifndef BinaryPath
#define BinaryPath "..\target\release\stet.exe"
#endif
#ifndef OutputPath
#define OutputPath "..\release"
#endif
#ifndef StetVersion
#error StetVersion must be supplied by scripts/package.ps1
#endif

[Setup]
AppId={{89B73B6A-C935-4A69-A7C2-9A816A40D273}
AppName=Stet
AppVersion={#StetVersion}
AppVerName=Stet {#StetVersion}
DefaultDirName={autopf}\Stet
DefaultGroupName=Stet
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputPath}
OutputBaseFilename=Stet-Setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableWelcomePage=no
SetupIconFile=..\assets\stet.ico
WizardImageFile=..\assets\stet-wizard.png
WizardSmallImageFile=..\assets\stet-wizard-small.png
ChangesEnvironment=yes
UninstallDisplayIcon={app}\stet.exe
CloseApplications=no
VersionInfoVersion={#StetVersion}.0
VersionInfoDescription=Stet Setup

[Files]
Source: "{#BinaryPath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\RUST_NOTICES.html"; DestDir: "{app}"; Flags: ignoreversion

[Code]
const
  EnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  OwnershipKey = 'Software\Stet\Installer';

#include "path.iss"

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  OwnedPath: String;
begin
  Result := '';
  if Pos(';', ExpandConstant('{app}')) > 0 then begin
    Result := 'The installation directory cannot contain semicolons because it is added to PATH.';
    Exit;
  end;
  if RegQueryStringValue(HKLM, OwnershipKey, 'AddedPath', OwnedPath) and
    (NormalizeEntry(OwnedPath) <> NormalizeEntry(ExpandConstant('{app}'))) then
    Result := 'Upgrade to the existing installation directory, or uninstall first.';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Value, InstallPath, OwnedPath: String;
begin
  if CurStep <> ssPostInstall then Exit;
  InstallPath := ExpandConstant('{app}');
  if not RegQueryStringValue(HKLM, EnvironmentKey, 'Path', Value) then
    RaiseException('Cannot read the system PATH; no PATH changes were made.');
  RegQueryStringValue(HKLM, OwnershipKey, 'AddedPath', OwnedPath);
  if (OwnedPath <> '') and (NormalizeEntry(OwnedPath) <> NormalizeEntry(InstallPath)) then
    RaiseException('Upgrade to the existing installation directory, or uninstall first.');
  if not ContainsPath(Value, InstallPath) then begin
    if not RegWriteStringValue(HKLM, OwnershipKey, 'AddedPath', InstallPath) then
      RaiseException('Cannot record PATH ownership.');
    if not RegWriteExpandStringValue(HKLM, EnvironmentKey, 'Path', AppendPathEntry(Value, InstallPath)) then
      RaiseException('Cannot update the system PATH.');
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Value, OwnedPath, Updated: String;
begin
  if CurUninstallStep <> usUninstall then Exit;
  if not RegQueryStringValue(HKLM, OwnershipKey, 'AddedPath', OwnedPath) then Exit;
  if NormalizeEntry(OwnedPath) <> NormalizeEntry(ExpandConstant('{app}')) then Exit;
  if not RegQueryStringValue(HKLM, EnvironmentKey, 'Path', Value) then Exit;
  Updated := RemovePathEntry(Value, OwnedPath);
  if not RegWriteExpandStringValue(HKLM, EnvironmentKey, 'Path', Updated) then
    RaiseException('Cannot remove Stet from PATH.');
  RegDeleteValue(HKLM, OwnershipKey, 'AddedPath');
  RegDeleteKeyIfEmpty(HKLM, OwnershipKey);
end;
