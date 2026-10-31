; ShellMusic — instalador de Windows (Inno Setup 6)
;
; Compilación en CI (release.yml): la versión se inyecta con /DAppVersion
; (mostrada en el asistente) y /DVersionInfo (x.x.x.x, para los metadatos
; del exe). Sin esas defines compila como 0.0.0-dev; para una versión real
; usa siempre:
;
;   iscc /DAppVersion=0.3.1 /DVersionInfo=0.3.1.0 installer\smusic.iss
;
; Instalación por usuario (sin admin) bajo %LOCALAPPDATA%\Programs\ShellMusic
; para que el paso final de winget corra sin elevar.

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef VersionInfo
  #define VersionInfo "0.0.0.0"
#endif

[Setup]
AppId={{29E2D653-A5EF-4433-8BC7-DF8AFC451752}
AppName=ShellMusic
AppVersion={#AppVersion}
VersionInfoVersion={#VersionInfo}
AppPublisher=devFuentesQuijon
AppPublisherURL=https://github.com/devFuentesQuijon/ShellMusic
AppSupportURL=https://github.com/devFuentesQuijon/ShellMusic/issues
AppUpdatesURL=https://github.com/devFuentesQuijon/ShellMusic/releases
DefaultDirName={localappdata}\Programs\ShellMusic
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFileName=smusic-windows-x86_64-setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes

[Files]
Source: "..\target\release\smusic.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\smusic-mcp.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\ShellMusic"; Filename: "{app}\smusic.exe"; Comment: "ShellMusic: reproductor musical en la terminal"

[Run]
; Instalación por usuario (PrivilegesRequired=lowest), así que winget corre
; sin elevar. Cada paquete solo se instala si no está ya en el PATH.
Filename: "powershell.exe"; \
  Parameters: "-NoProfile -Command ""$m=Get-Command mpv -ErrorAction SilentlyContinue; $y=Get-Command yt-dlp -ErrorAction SilentlyContinue; if(-not $m){winget install -e --id mpv --accept-source-agreements --accept-package-agreements}; if(-not $y){winget install -e --id yt-dlp --accept-source-agreements --accept-package-agreements}"""; \
  Description: "Instalar dependencias (mpv y yt-dlp) con winget"; \
  Flags: postinstall runasoriginaluser

[Code]
// Añade {app} al PATH del usuario (HKCU\Environment) una sola vez.
// ChangesEnvironment=yes hace que Windows notifique el cambio a las apps.
procedure CurStepChanged(CurStep: TSetupStep);
var
  Orig, Dir: String;
begin
  if CurStep <> ssPostInstall then exit;
  Dir := ExpandConstant('{app}');
  if RegQueryStringValue(HKCU, 'Environment', 'Path', Orig) then begin
    if Orig = '' then
      RegWriteExpandStringValue(HKCU, 'Environment', 'Path', Dir)
    else if Pos(';' + Dir + ';', ';' + Orig + ';') = 0 then
      RegWriteExpandStringValue(HKCU, 'Environment', 'Path', Orig + ';' + Dir);
  end else
    RegWriteExpandStringValue(HKCU, 'Environment', 'Path', Dir);
end;

// Desinstalación: quita {app} del PATH del usuario (lógica de modpath.iss).
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Path, Dir: String;
begin
  if CurUninstallStep <> usPostUninstall then exit;
  Dir := ExpandConstant('{app}');
  if RegQueryStringValue(HKCU, 'Environment', 'Path', Path) then begin
    StringChangeEx(Path, ';' + Dir, '', True);
    StringChangeEx(Path, Dir + ';', '', True);
    StringChangeEx(Path, Dir, '', True);
    RegWriteExpandStringValue(HKCU, 'Environment', 'Path', Path);
  end;
end;
