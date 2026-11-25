; MiSTerKVMS Capture for Windows: installs the capture server as a service.
;
; The installer is a thin wrapper: it runs the program's own
; `service install`, the same code as the setup page's "Run in the
; background" and its updates (server/src/service.rs). That copies the
; program to %ProgramFiles%\MiSTerKVMS\Capture, registers the service
; (LocalSystem, starts with the PC), adds the firewall rules and starts it;
; on an upgrade it first stops the old service and makes sure its process
; is gone. Uninstalling runs `service uninstall`. Settings in
; %ProgramData%\MiSTerKVMS\Capture stay.
;
; CI builds it (release.yml, build-windows):
;   ISCC.exe /DAppVersion=1.0.32 /DSourceExe=path\to\misterkvms-capture.exe /Obuild capture.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\target\release\misterkvms-capture.exe"
#endif

[Setup]
AppId={{6F2C3E8A-4B1D-4C7E-9A55-2D8E61B0C4F7}
AppName=MiSTerKVMS Capture
AppVersion={#AppVersion}
AppVerName=MiSTerKVMS Capture {#AppVersion}
AppPublisher=MiSTerKVMS
AppPublisherURL=https://github.com/danifunker/MiSTerKVMS
; Where `service install` puts the program; fixed, as the service expects it there.
DefaultDirName={autopf}\MiSTerKVMS\Capture
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputBaseFilename=MiSTerKVMS-Capture-Setup-{#AppVersion}-win-x64
UninstallDisplayName=MiSTerKVMS Capture
UninstallDisplayIcon={app}\misterkvms-capture.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; The service holds the program while it runs; `service install` deals with it.
CloseApplications=no

[Files]
; Unpacked to a temporary folder: `service install` copies itself into place.
Source: "{#SourceExe}"; DestDir: "{tmp}"; DestName: "misterkvms-capture.exe"; Flags: ignoreversion deleteafterinstall

[Run]
Filename: "http://localhost:5971"; Description: "Open the setup page (add a stream for each capture card or JetKVM)"; Flags: shellexec postinstall nowait skipifsilent

[UninstallRun]
Filename: "{app}\misterkvms-capture.exe"; Parameters: "service uninstall --config ""{commonappdata}\MiSTerKVMS\Capture"""; Flags: runhidden waituntilterminated; RunOnceId: "RemoveService"

[Code]
function ConfigDir: String;
begin
  Result := ExpandConstant('{commonappdata}\MiSTerKVMS\Capture');
end;

// Install (or update) the service with the program's own code, and say why
// if it didn't work (it writes the reason to service-error.txt).
procedure CurStepChanged(CurStep: TSetupStep);
var
  Code: Integer;
  Reason: AnsiString;
begin
  if CurStep <> ssPostInstall then
    Exit;
  WizardForm.StatusLabel.Caption := 'Installing the MiSTerKVMS Capture service...';
  if Exec(ExpandConstant('{tmp}\misterkvms-capture.exe'), 'service install --config "' + ConfigDir + '"', '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0) then
    Exit;
  if not LoadStringFromFile(ConfigDir + '\service-error.txt', Reason) then
    Reason := 'exit code ' + IntToStr(Code);
  MsgBox('MiSTerKVMS Capture was copied, but its service didn''t start: ' + Reason + #13#10#13#10 +
    'You can try again from the setup page (http://localhost:5971 after starting the program), or run this installer again.',
    mbError, MB_OK);
end;
