use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::ffi::OsStr;

use anyhow::{Context as AnyhowContext, Result, anyhow, bail};

use super::manifest::UpdateAsset;
use super::sync::sync_dir_filtered;

pub(super) fn check_dependencies() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        ensure_command("hdiutil")?;
    }

    #[cfg(target_os = "linux")]
    {
        ensure_command("tar")?;
    }

    Ok(())
}

pub(super) fn install_asset(
    asset: &UpdateAsset,
    downloaded_asset: &Path,
    download_dir: &Path,
) -> Result<Option<PathBuf>> {
    match env::consts::OS {
        "macos" => {
            install_macos_update(downloaded_asset, download_dir)?;
            Ok(None)
        }
        "linux" => install_linux_update(downloaded_asset, download_dir),
        "windows" => {
            if asset.kind != "inno" {
                bail!("expected an Inno Setup installer for Windows updates");
            }
            Ok(None)
        }
        os => bail!("auto-updates are not supported on {os}"),
    }
}

fn ensure_command(command: &str) -> Result<()> {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
        .with_context(|| format!("could not find required command `{command}`"))
}

pub(super) fn update_dir(version: &str) -> Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .context("could not find local data directory")?
        .join("Monocurl")
        .join("updates")
        .join(version);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to clear old update directory {}", dir.display()))?;
    }
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create update directory {}", dir.display()))?;
    Ok(dir)
}

#[cfg(target_os = "macos")]
fn install_macos_update(downloaded_dmg: &Path, temp_dir: &Path) -> Result<()> {
    let app_path = current_macos_app_bundle()?;
    let app_name = app_path
        .file_name()
        .ok_or_else(|| anyhow!("invalid app bundle path {}", app_path.display()))?;
    let mount_root = temp_dir.join("mount");
    fs::create_dir_all(&mount_root)
        .with_context(|| format!("failed to create {}", mount_root.display()))?;

    let output = Command::new("hdiutil")
        .args(["attach", "-nobrowse"])
        .arg(downloaded_dmg)
        .arg("-mountroot")
        .arg(&mount_root)
        .output()
        .context("failed to mount update disk image")?;
    ensure_output_success(output, "mount update disk image")?;

    let (mount_path, mounted_app) = find_mounted_app(&mount_root, app_name)?;
    let copy_result = sync_dir_filtered(&mounted_app, &app_path, macos_finder_icon_file);

    let detach_result = Command::new("hdiutil")
        .args(["detach", "-force"])
        .arg(&mount_path)
        .output()
        .context("failed to detach update disk image")
        .and_then(|output| ensure_output_success(output, "detach update disk image"));

    copy_result.and(detach_result)
}

#[cfg(not(target_os = "macos"))]
fn install_macos_update(_downloaded_dmg: &Path, _temp_dir: &Path) -> Result<()> {
    bail!("macOS updates are only supported on macOS")
}

#[cfg(target_os = "macos")]
fn current_macos_app_bundle() -> Result<PathBuf> {
    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    current_exe
        .ancestors()
        .find(|path| {
            path.extension()
                .and_then(OsStr::to_str)
                .is_some_and(|extension| extension == "app")
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("auto-updates require Monocurl to be running from an .app bundle"))
}

#[cfg(target_os = "macos")]
fn find_mounted_app(mount_root: &Path, app_name: &OsStr) -> Result<(PathBuf, PathBuf)> {
    for entry in fs::read_dir(mount_root)
        .with_context(|| format!("failed to read {}", mount_root.display()))?
    {
        let entry = entry?;
        let mount_path = entry.path();
        if !mount_path.is_dir() {
            continue;
        }

        let app = mount_path.join(app_name);
        if app.is_dir() {
            return Ok((mount_path, app));
        }
    }

    bail!(
        "mounted update disk image did not contain {}",
        Path::new(app_name).display()
    )
}

#[cfg(target_os = "macos")]
fn macos_finder_icon_file(relative_path: &Path) -> bool {
    relative_path.file_name().is_some_and(|name| {
        let name = name.to_string_lossy();
        name.starts_with("Icon") && name.chars().count() == 5
    })
}

#[cfg(target_os = "linux")]
fn install_linux_update(downloaded_tar_gz: &Path, temp_dir: &Path) -> Result<Option<PathBuf>> {
    let extracted = temp_dir.join("extract");
    fs::create_dir_all(&extracted)
        .with_context(|| format!("failed to create {}", extracted.display()))?;

    let output = Command::new("tar")
        .arg("-xzf")
        .arg(downloaded_tar_gz)
        .arg("-C")
        .arg(&extracted)
        .output()
        .context("failed to extract Linux update")?;
    ensure_output_success(output, "extract Linux update")?;

    let app_dir = extracted.join("monocurl.app");
    if !app_dir.join("bin/monocurl").is_file() {
        bail!("Linux update did not contain monocurl.app/bin/monocurl");
    }

    let current_exe = env::current_exe().context("failed to resolve current executable")?;
    let install_prefix = linux_install_prefix(&current_exe)?;
    fs::create_dir_all(&install_prefix)
        .with_context(|| format!("failed to create {}", install_prefix.display()))?;

    let installed_app = install_prefix.join("monocurl.app");
    sync_dir_filtered(&app_dir, &installed_app, |_| false)?;
    refresh_linux_registration(&install_prefix, &installed_app)?;
    Ok(Some(installed_app.join("bin/monocurl")))
}

#[cfg(not(target_os = "linux"))]
fn install_linux_update(_downloaded_tar_gz: &Path, _temp_dir: &Path) -> Result<Option<PathBuf>> {
    bail!("Linux updates are only supported on Linux")
}

#[cfg(target_os = "linux")]
fn linux_install_prefix(current_exe: &Path) -> Result<PathBuf> {
    let is_bundled_layout = current_exe.file_name() == Some(OsStr::new("monocurl"))
        && current_exe.parent().and_then(Path::file_name) == Some(OsStr::new("bin"))
        && current_exe
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            == Some(OsStr::new("monocurl.app"));

    if is_bundled_layout
        && let Some(prefix) = current_exe
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
    {
        return Ok(prefix.to_path_buf());
    }

    Ok(dirs::home_dir()
        .context("could not find home directory")?
        .join(".local"))
}

#[cfg(target_os = "linux")]
fn refresh_linux_registration(install_prefix: &Path, app_dir: &Path) -> Result<()> {
    let Some(home_local) = dirs::home_dir().map(|home| home.join(".local")) else {
        return Ok(());
    };
    if install_prefix != home_local {
        return Ok(());
    }

    let bin_dir = install_prefix.join("bin");
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("failed to create {}", bin_dir.display()))?;
    let link_path = bin_dir.join("monocurl");
    let _ = fs::remove_file(&link_path);
    std::os::unix::fs::symlink(app_dir.join("bin/monocurl"), &link_path)
        .with_context(|| format!("failed to create {}", link_path.display()))?;

    let desktop_src = app_dir.join("share/applications/com.enigmadux.monocurl.desktop");
    if desktop_src.is_file() {
        let desktop_dir = install_prefix.join("share/applications");
        fs::create_dir_all(&desktop_dir)
            .with_context(|| format!("failed to create {}", desktop_dir.display()))?;
        let desktop_dst = desktop_dir.join("com.enigmadux.monocurl.desktop");
        let content = fs::read_to_string(&desktop_src)
            .with_context(|| format!("failed to read {}", desktop_src.display()))?;
        let rewritten = rewrite_linux_desktop_file(&content, app_dir);
        fs::write(&desktop_dst, rewritten)
            .with_context(|| format!("failed to write {}", desktop_dst.display()))?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn rewrite_linux_desktop_file(content: &str, app_dir: &Path) -> String {
    let mut rewritten = String::new();
    for line in content.lines() {
        if line.starts_with("Exec=") {
            rewritten.push_str(&format!(
                "Exec={} %F",
                app_dir.join("bin/monocurl").display()
            ));
        } else if line.starts_with("Icon=") {
            rewritten.push_str(&format!(
                "Icon={}",
                app_dir
                    .join("share/icons/hicolor/512x512/apps/monocurl.png")
                    .display()
            ));
        } else {
            rewritten.push_str(line);
        }
        rewritten.push('\n');
    }
    rewritten
}

fn ensure_output_success(output: Output, action: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(anyhow!(
        "{action} failed with status {}: {}{}{}",
        output.status,
        stderr.trim(),
        if stderr.trim().is_empty() || stdout.trim().is_empty() {
            ""
        } else {
            "\n"
        },
        stdout.trim()
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn spawn_windows_update_after_exit(installer: &Path) -> Result<()> {
    let app_exe = env::current_exe().context("failed to resolve current executable")?;
    let app_working_dir = env::current_dir().context("failed to resolve current directory")?;
    let script = format!(
        r#"
$pidToWaitFor = {}
$installer = '{}'
$appExe = '{}'
$appWorkingDir = '{}'
Wait-Process -Id $pidToWaitFor -ErrorAction SilentlyContinue
$install = Start-Process -FilePath $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/MERGETASKS=!desktopicon') -Wait -PassThru
if ($install.ExitCode -eq 0 -and (Test-Path -LiteralPath $appExe)) {{
    Start-Process -FilePath $appExe -WorkingDirectory $appWorkingDir
}}
"#,
        std::process::id(),
        powershell_string(installer),
        powershell_string(&app_exe),
        powershell_string(&app_working_dir)
    );

    Command::new("powershell.exe")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command"])
        .arg(script)
        .spawn()
        .context("failed to spawn PowerShell updater launcher")?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn powershell_string(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}
