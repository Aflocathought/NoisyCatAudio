; Build through scripts/build-windows-installer.ps1. Keep AppId and the CLAP
; filename stable: they identify upgrades in Windows and existing DAW projects.
#if VER < 0x06070300
  #error Inno Setup 6.7.3 or newer is required
#endif
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PluginFile
  #error PluginFile is required
#endif
#define ProductId "AflocatAudio.SpectralResonator.CLAP"
#define ProductName "Specatral Resonator"
#define ProductKey "Software\AflocatAudio\SpectralResonator\Installer"
#ifdef TestId
  #define ProductId "AflocatAudio.SR.Test." + TestId
  #define ProductName ProductName + " (Installer Test)"
  #define ProductKey "Software\AflocatAudio\InstallerTests\" + TestId
#endif

[Setup]
AppId={#ProductId}
AppName={#ProductName}
AppVersion={#AppVersion}
AppPublisher=Noisy Cat Audio
AppPublisherURL=https://github.com/Aflocathought/NoisyCatAudio
AppSupportURL=https://github.com/Aflocathought/NoisyCatAudio/issues
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Specatral Resonator CLAP installer
#ifdef TestId
DefaultDirName={#TestRoot}\support
#else
DefaultDirName={autopf}\AflocatAudio\Spectral Resonator
#endif
DisableDirPage=yes
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\spectral-resonator.ico
SetupIconFile={#IconFile}
OutputDir={#OutputDir}
OutputBaseFilename=SpectralResonator-{#AppVersion}-Windows-x64-Setup
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UsePreviousPrivileges=yes
UsePreviousAppDir=yes
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
SetupMutex={#ProductId}.Setup
CloseApplications=no
RestartApplications=no
AllowCancelDuringInstall=no
UninstallLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "chinesesimp"; MessagesFile: "ChineseSimplified.isl"

[CustomMessages]
english.PluginTitle=CLAP plug-in folder
english.PluginDescription=Choose where your audio host will find Specatral Resonator.
english.PluginPrompt=Select the CLAP scan folder. If you choose a custom folder, add it to your host's plug-in locations. Close audio hosts before installing or updating.
english.PluginFolder=CLAP folder:
english.SupportFolder=Uninstaller and icon:
english.RuntimeNote=Microsoft Visual C++ x64 runtime is missing. It will be installed first; Windows may request administrator approval. This shared runtime is retained on uninstall.
english.RuntimeFailed=Microsoft Visual C++ x64 runtime installation failed or was cancelled (code %1). No plug-in files were installed. Retry Setup and allow the Microsoft runtime installation.
english.RuntimeRestart=The Microsoft runtime requires a Windows restart. Restart Windows, then run this installer again.
english.UpdateFolder=An existing installation was found. This update uses its folder to avoid duplicate plug-ins. To move it, uninstall first, then reinstall; shared preferences are retained.
english.OtherScope=This plug-in is already installed for a different user scope. Run Setup in the same installation mode, or uninstall the existing installation first.
english.Downgrade=A newer version (%1) is already installed. This package is %2. Uninstall first if you intentionally want an older version; preferences will be retained.
english.BadState=The existing installation record is incomplete. Uninstall it using Windows Settings before installing again.
english.MoveBlocked=Updates must use the existing CLAP folder: %1. To move the plug-in, uninstall first, then reinstall.
english.BadFolder=Choose an absolute local folder path, not a drive root or a file.
english.Locked=The plug-in file is in use or is not writable:%n%1%n%nClose audio hosts, then try again. No host will be closed automatically.
english.Unmanaged=A file already exists at:%n%1%n%nInstalling here will replace this file and make it part of this installation (including removal on uninstall). Continue?
english.Finish=Specatral Resonator is installed. Open your audio host and rescan CLAP plug-ins. For a custom folder, add the selected folder to the host's plug-in locations. Remove any older manually copied version from other scan folders to avoid duplicates.
chinesesimp.PluginTitle=CLAP 插件目录
chinesesimp.PluginDescription=选择音频宿主扫描猫振器（Specatral Resonator）的位置。
chinesesimp.PluginPrompt=请选择 CLAP 扫描目录。若使用自定义目录，请将其加入宿主的插件位置。安装或更新前请关闭音频宿主。
chinesesimp.PluginFolder=CLAP 目录：
chinesesimp.SupportFolder=卸载器和图标：
chinesesimp.RuntimeNote=缺少 Microsoft Visual C++ x64 运行库。将先安装该组件，Windows 可能请求管理员授权。卸载插件时会保留这一共享运行库。
chinesesimp.RuntimeFailed=Microsoft Visual C++ x64 运行库安装失败或被取消（代码 %1）。尚未安装插件文件。请重试，并允许安装微软运行库。
chinesesimp.RuntimeRestart=微软运行库需要重启 Windows。请重启后重新运行本安装器。
chinesesimp.UpdateFolder=已找到现有安装。本次更新沿用原目录，避免产生重复插件。如需迁移，请先卸载再重新安装；共享偏好会保留。
chinesesimp.OtherScope=已在另一种用户范围下安装此插件。请使用相同的安装模式运行安装器，或先卸载现有安装。
chinesesimp.Downgrade=已安装的版本（%1）比本安装包（%2）更新。如需降级，请先卸载再安装；偏好设置会保留。
chinesesimp.BadState=现有安装记录不完整。请先从 Windows 设置中卸载，再重新安装。
chinesesimp.MoveBlocked=更新必须沿用已有 CLAP 目录：%1。如需迁移，请先卸载再安装。
chinesesimp.BadFolder=请选择本地磁盘中的绝对目录路径，不能选择盘符根目录或文件。
chinesesimp.Locked=插件文件正在使用或无法写入：%n%1%n%n请关闭音频宿主后重试。安装器不会自动关闭宿主。
chinesesimp.Unmanaged=此位置已有文件：%n%1%n%n继续安装会替换该文件，并由本安装器管理（卸载时也会删除）。是否继续？
chinesesimp.Finish=猫振器（Specatral Resonator） 已安装。请打开音频宿主并重新扫描 CLAP 插件；使用自定义目录时，请将目录加入宿主的插件位置。若其他扫描目录中有以前手动复制的版本，请自行移除，以免出现重复插件。

[Files]
; No wildcard deletion or shared-directory ownership. Never queue replacement
; for reboot: an in-use DLL must remain intact until its host releases it.
Source: "{#PluginFile}"; DestDir: "{code:GetPluginDir}"; DestName: "my_spectral_resonator.clap"; Flags: ignoreversion
Source: "{#IconFile}"; DestDir: "{app}"; DestName: "spectral-resonator.ico"; Flags: ignoreversion
Source: "{#VcRuntimeFile}"; DestName: "vc_redist.x64.exe"; Flags: dontcopy nocompression

[Registry]
Root: HKA; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "PluginDir"; ValueData: "{code:GetPluginDir}"; Flags: uninsdeletekey
Root: HKA; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "Version"; ValueData: "{#AppVersion}"

[Code]
var
  PluginPage: TInputDirWizardPage;
  InstalledDir, InstalledVersion: String;
  AdoptedFile: String;

function CreateFileW(Name: String; Access, Share: Cardinal; Security: Integer;
  Creation, Flags: Cardinal; Template: Integer): THandle;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseHandle(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

function NormalPath(Path: String): String;
begin
  Result := RemoveBackslashUnlessRoot(ExpandFileName(Path));
end;

function GetPluginDir(Param: String): String;
begin
  Result := NormalPath(PluginPage.Values[0]);
end;

function RuntimePresent: Boolean;
var
  VersionText: String;
  Version, Minimum: Int64;
begin
#ifdef TestId
  if ExpandConstant('{param:TESTMISSINGRUNTIME|0}') = '1' then begin
    Result := False;
    Exit;
  end;
#endif
  // The PE import table uses VCRUNTIME140's original 2015 ABI. UCRT is part
  // of supported Windows 10/11; newer compatible VC runtimes need no repair.
  Result := GetVersionNumbersString(ExpandConstant('{sys}\vcruntime140.dll'), VersionText);
  if Result then begin
    StrToVersion('14.0.0.0', Minimum);
    Result := StrToVersion(VersionText, Version);
    if Result then Result := ComparePackedVersion(Version, Minimum) >= 0;
  end;
  Log('VC++ x64 runtime present: ' + IntToStr(Ord(Result)) + ' version: ' + VersionText);
end;

function FileAvailable(Path: String): Boolean;
var
  Handle: THandle;
begin
  Result := not FileExists(Path);
  if Result then Exit;
  // Exclusive read/write fails for a loaded DLL or a host's open file handle.
  Handle := CreateFileW(Path, $C0000000, 0, 0, 3, $80, 0);
  Result := Handle <> THandle(-1);
  if Result then CloseHandle(Handle);
end;

function InitializeSetup: Boolean;
var
  OtherRoot: Integer;
  OldVersion, NewVersion: Int64;
begin
  Result := False;
  if IsAdminInstallMode then OtherRoot := HKCU64 else OtherRoot := HKLM64;
  if RegKeyExists(OtherRoot, '{#ProductKey}') then begin
    SuppressibleMsgBox(CustomMessage('OtherScope'), mbError, MB_OK, IDOK);
    Exit;
  end;
  if RegKeyExists(HKA, '{#ProductKey}') then begin
    if not RegQueryStringValue(HKA, '{#ProductKey}', 'PluginDir', InstalledDir) or
       not RegQueryStringValue(HKA, '{#ProductKey}', 'Version', InstalledVersion) or
       (InstalledDir = '') or not StrToVersion(InstalledVersion, OldVersion) then begin
      SuppressibleMsgBox(CustomMessage('BadState'), mbError, MB_OK, IDOK);
      Exit;
    end;
    StrToVersion('{#AppVersion}', NewVersion);
    if ComparePackedVersion(OldVersion, NewVersion) > 0 then begin
      SuppressibleMsgBox(FmtMessage(CustomMessage('Downgrade'), [InstalledVersion,
        '{#AppVersion}']), mbError, MB_OK, IDOK);
      Exit;
    end;
  end;
  Result := True;
end;

procedure InitializeWizard;
var
  RequestedDir, DefaultDir: String;
begin
  PluginPage := CreateInputDirPage(wpSelectDir, CustomMessage('PluginTitle'),
    CustomMessage('PluginDescription'), CustomMessage('PluginPrompt'), False, '');
  PluginPage.Add(CustomMessage('PluginFolder'));
#ifdef TestId
  DefaultDir := '{#TestRoot}\clap';
#else
  DefaultDir := ExpandConstant('{autocf}\CLAP');
#endif
  RequestedDir := ExpandConstant('{param:CLAPDIR|}');
  if InstalledDir <> '' then begin
    DefaultDir := InstalledDir;
    PluginPage.Edits[0].ReadOnly := True;
    PluginPage.Buttons[0].Enabled := False;
    PluginPage.SubCaptionLabel.Caption := CustomMessage('UpdateFolder');
  end else if RequestedDir <> '' then DefaultDir := RequestedDir;
  PluginPage.Values[0] := DefaultDir;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpReady then begin
    WizardForm.ReadyMemo.ScrollBars := ssVertical;
    WizardForm.ReadyMemo.WordWrap := True;
  end;
  // [Messages] values do not expand {cm:...}; assign the localized guidance
  // after Inno has selected its finished-page variant (with/without shortcuts).
  if CurPageID = wpFinished then
    WizardForm.FinishedLabel.Caption := CustomMessage('Finish');
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfo, MemoDirInfo,
  MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
begin
  // Make the actual plug-in destination reviewable; {app} only holds support
  // files, and the default Inno memo would otherwise show just that directory.
  Result := CustomMessage('PluginFolder') + NewLine + Space + GetPluginDir('') +
    NewLine + NewLine + CustomMessage('SupportFolder') + NewLine + Space + WizardDirValue;
  if not RuntimePresent then
    Result := Result + NewLine + NewLine + CustomMessage('RuntimeNote');
end;

function CheckDestination: String;
var
  Dir, Path, RequestedDir: String;
begin
  Result := '';
  Dir := GetPluginDir('');
  RequestedDir := ExpandConstant('{param:CLAPDIR|}');
  if (InstalledDir <> '') and (RequestedDir <> '') and
      (CompareText(NormalPath(RequestedDir), NormalPath(InstalledDir)) <> 0) then begin
    Result := FmtMessage(CustomMessage('MoveBlocked'), [InstalledDir]);
    Exit;
  end;
  // Accept local absolute paths with Unicode/spaces; reject network paths,
  // drive roots, and relative paths before ExpandFileName could hide them.
  if (Length(PluginPage.Values[0]) < 4) or
     (Copy(PluginPage.Values[0], 2, 2) <> ':\') or
     (Length(Dir) <= 3) or FileExists(Dir) then begin
    Result := CustomMessage('BadFolder');
    Exit;
  end;
#ifdef TestId
  // Test installers must never write payload/support files outside their run.
  if (Pos(Lowercase('{#TestRoot}\'), Lowercase(AddBackslash(Dir))) <> 1) or
     (Pos(Lowercase('{#TestRoot}\'), Lowercase(AddBackslash(NormalPath(WizardDirValue)))) <> 1) then begin
    Result := 'Test installer path is outside TestRoot.';
    Exit;
  end;
#endif
  Path := AddBackslash(Dir) + 'my_spectral_resonator.clap';
  if not FileAvailable(Path) then begin
    Result := FmtMessage(CustomMessage('Locked'), [Path]);
    Exit;
  end;
  if (InstalledDir = '') and FileExists(Path) and (AdoptedFile <> Path) then begin
    if SuppressibleMsgBox(FmtMessage(CustomMessage('Unmanaged'), [Path]),
        mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) <> IDYES then begin
      Result := 'Existing unmanaged plug-in was not replaced.';
      Exit;
    end;
    AdoptedFile := Path;
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Error: String;
begin
  Result := True;
  if CurPageID = PluginPage.ID then begin
    Error := CheckDestination;
    Result := Error = '';
    if not Result then SuppressibleMsgBox(Error, mbError, MB_OK, IDOK);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ExitCode: Integer;
  Started: Boolean;
begin
  // Run for both interactive and silent installations, including repair.
  Result := CheckDestination;
  if (Result <> '') or RuntimePresent then Exit;
#ifdef TestId
  // Test products must never install or modify a system-wide prerequisite.
  Result := 'Test build refuses system runtime installation.';
#else
  ExtractTemporaryFile('vc_redist.x64.exe');
  if IsAdminInstallMode then
    Started := Exec(ExpandConstant('{tmp}\vc_redist.x64.exe'),
      '/install /passive /norestart', '', SW_SHOW, ewWaitUntilTerminated, ExitCode)
  else
    Started := ShellExec('runas', ExpandConstant('{tmp}\vc_redist.x64.exe'),
      '/install /passive /norestart', '', SW_SHOW, ewWaitUntilTerminated, ExitCode);
  if Started and (ExitCode = 3010) then begin
    NeedsRestart := True;
    Result := CustomMessage('RuntimeRestart');
  end else if not Started or (ExitCode <> 0) then
    Result := FmtMessage(CustomMessage('RuntimeFailed'), [IntToStr(ExitCode)])
  else if not RuntimePresent then
    Result := FmtMessage(CustomMessage('RuntimeFailed'), ['runtime not detected']);
#endif
end;

function UninstallFileAvailable: Boolean;
var
  Dir, Path: String;
begin
  Result := False;
  if not RegQueryStringValue(HKA, '{#ProductKey}', 'PluginDir', Dir) then begin
    SuppressibleMsgBox(CustomMessage('BadState'), mbError, MB_OK, IDOK);
    Exit;
  end;
  Path := AddBackslash(Dir) + 'my_spectral_resonator.clap';
  Result := FileAvailable(Path);
  if not Result then
    SuppressibleMsgBox(FmtMessage(CustomMessage('Locked'), [Path]), mbError, MB_OK, IDOK);
end;

function InitializeUninstall: Boolean;
begin
  Result := UninstallFileAvailable;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // Recheck after the confirmation page in case a host was opened meanwhile.
  if (CurUninstallStep = usUninstall) and not UninstallFileAvailable then
    RaiseException('Plug-in is in use; uninstall cancelled.');
end;
