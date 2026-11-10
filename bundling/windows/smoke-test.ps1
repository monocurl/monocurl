param(
  [Parameter(Mandatory = $true)]
  [string]$Installer
)

# Installs the app with the release installer the way a user would and fails
# unless it runs every default scene and stays up as a GUI. No frame export:
# the renderer needs vulkan, which the runner does not have.

$ErrorActionPreference = "Stop"

$Work = Join-Path ([System.IO.Path]::GetTempPath()) ("monocurl-smoke-" + [System.Guid]::NewGuid().ToString("N"))
$InstallDir = Join-Path $Work "install"
New-Item -ItemType Directory -Force $Work | Out-Null

$setup = Start-Process -FilePath $Installer -Wait -PassThru `
  -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/DIR=`"$InstallDir`""
if ($setup.ExitCode -ne 0) { throw "installer exited with $($setup.ExitCode)" }

$Bin = Join-Path $InstallDir "monocurl.exe"
$Scenes = Join-Path $InstallDir "assets\default_scenes"
if (-not (Test-Path $Bin)) { throw "installed binary not found: $Bin" }

$scenesRun = 0
foreach ($scene in Get-ChildItem -Path $Scenes -Filter "*.mcs") {
  Write-Host "[smoke] stepping $($scene.Name)"
  $out = Join-Path $Work "out.txt"
  $err = Join-Path $Work "err.txt"
  $run = Start-Process -FilePath $Bin -Wait -PassThru -NoNewWindow `
    -ArgumentList "video", "`"$($scene.FullName)`"", "--list-slides" `
    -RedirectStandardOutput $out -RedirectStandardError $err
  if ($run.ExitCode -ne 0) {
    Get-Content $err | Write-Host
    throw "$($scene.Name) failed with exit code $($run.ExitCode)"
  }
  $scenesRun++
}
if ($scenesRun -eq 0) { throw "no default scenes found in $Scenes" }

$secs = 20
Write-Host "[smoke] launching the app for ${secs}s"
$app = Start-Process -FilePath $Bin -PassThru
Start-Sleep -Seconds $secs
if ($app.HasExited) { throw "the app exited with code $($app.ExitCode) during launch" }
Stop-Process -Id $app.Id -Force
Write-Host "[smoke] app stayed up"

Start-Process -FilePath (Join-Path $InstallDir "unins000.exe") -Wait `
  -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -ErrorAction SilentlyContinue
Write-Host "[ok] Windows smoke test passed"
