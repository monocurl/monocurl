use std::{
    env,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

use anyhow::{Context as AnyhowContext, Result, bail};
use gpui::*;
use semver::Version;

use crate::state::user_settings::UserSettings;

use self::{
    download::{download_asset, verify_asset},
    install::{check_dependencies, update_dir},
    manifest::{
        UpdateAsset, fetch_manifest, matching_asset, update_disabled_explanation,
        validate_asset_kind, version_is_newer,
    },
};

mod download;
mod install;
mod manifest;
mod sync;

pub const CURRENT_VERSION: &str = match option_env!("MONOCURL_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

const POLL_INTERVAL: Duration = Duration::from_secs(30 * 60);
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoUpdateStatus {
    Idle,
    Checking,
    Downloading { version: String },
    Installing { version: String },
    ReadyToRestart { version: String },
    Errored { message: String },
}

impl AutoUpdateStatus {
    pub fn ready_version(&self) -> Option<&str> {
        match self {
            Self::ReadyToRestart { version } => Some(version),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateCheckKind {
    Automatic,
    Manual,
}

#[derive(Default)]
struct GlobalAutoUpdater(Option<Entity<AutoUpdater>>);

impl Global for GlobalAutoUpdater {}

pub struct AutoUpdater {
    status: AutoUpdateStatus,
    current_version: Version,
    worker_running: bool,
    manual_prompt_window: Option<AnyWindowHandle>,
    ready_update_asset: Option<UpdateAsset>,
    ready_downloaded_asset: Option<PathBuf>,
    ready_download_dir: Option<PathBuf>,
    #[cfg(target_os = "windows")]
    pending_windows_installer: Option<PathBuf>,
    #[cfg(target_os = "windows")]
    _quit_subscription: Option<Subscription>,
}

enum WorkerMessage {
    Status(AutoUpdateStatus),
    Finished(Result<UpdateOutcome, String>),
}

enum UpdateOutcome {
    NoUpdate {
        version: String,
    },
    ReadyToRestart {
        version: String,
        asset: Option<UpdateAsset>,
        downloaded_asset: Option<PathBuf>,
        download_dir: Option<PathBuf>,
    },
}

impl AutoUpdater {
    pub fn init(cx: &mut App) {
        let updater = cx.new(Self::new);
        cx.set_global(GlobalAutoUpdater(Some(updater.clone())));
        updater.update(cx, |updater, cx| updater.start_automatic_polling(cx));
    }

    pub fn get(cx: &App) -> Option<Entity<Self>> {
        cx.has_global::<GlobalAutoUpdater>()
            .then(|| cx.global::<GlobalAutoUpdater>().0.clone())
            .flatten()
    }

    pub fn status(cx: &App) -> AutoUpdateStatus {
        Self::get(cx)
            .map(|updater| updater.read(cx).status.clone())
            .unwrap_or(AutoUpdateStatus::Idle)
    }

    pub fn check_for_updates(prompt_window: Option<AnyWindowHandle>, cx: &mut App) {
        if let Some(explanation) = update_disabled_explanation() {
            show_prompt(
                prompt_window.or_else(|| cx.active_window()),
                PromptLevel::Info,
                "Monocurl was installed by another updater.",
                Some(&explanation),
                cx,
            );
            return;
        }

        let Some(updater) = Self::get(cx) else {
            show_prompt(
                prompt_window.or_else(|| cx.active_window()),
                PromptLevel::Critical,
                "Could not check for updates",
                Some("The auto-updater has not been initialized."),
                cx,
            );
            return;
        };

        let _ = updater.update(cx, |updater, cx| {
            updater.start_check(UpdateCheckKind::Manual, prompt_window, cx);
        });
    }

    pub fn restart_from_update_status(cx: &mut App) {
        let mut ready = false;
        if let Some(updater) = Self::get(cx) {
            let _ = updater.update(cx, |updater, cx| {
                ready = updater.status.ready_version().is_some();
                if ready {
                    updater.restart_to_apply(cx);
                }
            });
        }

        if !ready {
            show_prompt(
                cx.active_window(),
                PromptLevel::Info,
                "No update is ready to install",
                Some("Use Help > Check for Updates to look for a new release."),
                cx,
            );
        }
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let current_version = Version::parse(CURRENT_VERSION).unwrap_or_else(|error| {
            log::warn!("unable to parse Monocurl version {CURRENT_VERSION:?}: {error}");
            Version::new(0, 0, 0)
        });
        #[cfg(not(target_os = "windows"))]
        let _ = cx;

        #[cfg(target_os = "windows")]
        let quit_subscription = Some(cx.on_app_quit(|this, _cx| {
            let installer = this.pending_windows_installer.clone();
            async move {
                if let Some(installer) = installer
                    && let Err(error) = install::spawn_windows_update_after_exit(&installer)
                {
                    log::error!("failed to spawn Windows updater: {error:#}");
                }
            }
        }));

        Self {
            status: AutoUpdateStatus::Idle,
            current_version,
            worker_running: false,
            manual_prompt_window: None,
            ready_update_asset: None,
            ready_downloaded_asset: None,
            ready_download_dir: None,
            #[cfg(target_os = "windows")]
            pending_windows_installer: None,
            #[cfg(target_os = "windows")]
            _quit_subscription: quit_subscription,
        }
    }

    fn start_automatic_polling(&mut self, cx: &mut Context<Self>) {
        if update_disabled_explanation().is_some() {
            return;
        }

        cx.spawn(async move |this, app| {
            loop {
                let should_check = app
                    .update(|cx| UserSettings::read(cx).auto_update)
                    .unwrap_or(false);
                if should_check
                    && this
                        .update(app, |this, cx| {
                            this.start_check(UpdateCheckKind::Automatic, None, cx);
                        })
                        .is_err()
                {
                    return;
                }

                app.background_executor().timer(POLL_INTERVAL).await;
            }
        })
        .detach();
    }

    fn start_check(
        &mut self,
        kind: UpdateCheckKind,
        prompt_window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        if kind == UpdateCheckKind::Automatic && self.status.ready_version().is_some() {
            return;
        }

        if self.worker_running {
            if kind == UpdateCheckKind::Manual {
                self.manual_prompt_window = prompt_window;
                self.prompt(
                    PromptLevel::Info,
                    "Update check already running",
                    Some("Monocurl is already checking for updates."),
                    cx,
                );
            }
            return;
        }

        self.worker_running = true;
        self.manual_prompt_window = prompt_window;
        self.status = AutoUpdateStatus::Checking;
        cx.notify();

        let current_version = self.current_version.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let status_tx = tx.clone();
            let result = run_update(current_version, move |status| {
                let _ = status_tx.send(WorkerMessage::Status(status));
            })
            .map_err(|error| format!("{error:#}"));
            let _ = tx.send(WorkerMessage::Finished(result));
        });

        cx.spawn(async move |this, app| {
            poll_worker(rx, kind, this, app).await;
        })
        .detach();
    }

    fn apply_worker_status(&mut self, status: AutoUpdateStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    fn apply_worker_finished(
        &mut self,
        result: Result<UpdateOutcome, String>,
        kind: UpdateCheckKind,
        cx: &mut Context<Self>,
    ) {
        self.worker_running = false;

        match result {
            Ok(UpdateOutcome::NoUpdate { version }) => {
                self.status = AutoUpdateStatus::Idle;
                self.ready_update_asset = None;
                self.ready_downloaded_asset = None;
                self.ready_download_dir = None;
                #[cfg(target_os = "windows")]
                {
                    self.pending_windows_installer = None;
                }
                if kind == UpdateCheckKind::Manual {
                    self.prompt(
                        PromptLevel::Info,
                        "Monocurl is up to date",
                        Some(&format!(
                            "You are running v{CURRENT_VERSION}. Latest is v{version}."
                        )),
                        cx,
                    );
                }
            }
            Ok(UpdateOutcome::ReadyToRestart {
                version,
                asset,
                downloaded_asset,
                download_dir,
            }) => {
                self.ready_update_asset = asset;
                self.ready_downloaded_asset = downloaded_asset;
                self.ready_download_dir = download_dir;
                #[cfg(target_os = "windows")]
                {
                    self.pending_windows_installer = None;
                }
                self.status = AutoUpdateStatus::ReadyToRestart {
                    version: version.clone(),
                };
                if kind == UpdateCheckKind::Manual {
                    self.prompt_ready_to_restart(version, cx);
                }
            }
            Err(message) => {
                if kind == UpdateCheckKind::Automatic {
                    log::info!("auto-update check failed: {message}");
                    self.status = AutoUpdateStatus::Idle;
                } else {
                    self.status = AutoUpdateStatus::Errored {
                        message: message.clone(),
                    };
                    self.prompt(
                        PromptLevel::Critical,
                        "Could not update Monocurl",
                        Some(&message),
                        cx,
                    );
                }
            }
        }

        cx.notify();
    }

    fn prompt(
        &mut self,
        level: PromptLevel,
        message: &str,
        detail: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let window = self
            .manual_prompt_window
            .take()
            .or_else(|| cx.active_window());
        show_prompt(window, level, message, detail, cx);
    }

    fn prompt_ready_to_restart(&mut self, version: String, cx: &mut Context<Self>) {
        let Some(window_handle) = self
            .manual_prompt_window
            .take()
            .or_else(|| cx.active_window())
        else {
            return;
        };

        let prompt = window_handle
            .update(cx, |_, window, cx| {
                window.prompt(
                    PromptLevel::Info,
                    &format!("Monocurl v{version} is ready"),
                    Some(
                        "Restart now installs the downloaded update and launches the new version. \
                         Choose Later to keep using this version and update manually.",
                    ),
                    &[
                        PromptButton::Ok("Restart now".into()),
                        PromptButton::Cancel("Later".into()),
                    ],
                    cx,
                )
            })
            .ok();

        if let Some(prompt) = prompt {
            cx.spawn(async move |this, app| {
                if prompt.await != Ok(0) {
                    return;
                }
                let _ = app.update(|cx| {
                    let _ = this.update(cx, |updater, cx| updater.restart_to_apply(cx));
                });
            })
            .detach();
        }
    }

    fn restart_to_apply(&mut self, cx: &mut Context<Self>) {
        let Some(version) = self.status.ready_version().map(str::to_string) else {
            self.status = AutoUpdateStatus::Errored {
                message: "No update is ready to install.".into(),
            };
            cx.notify();
            return;
        };

        let Some(asset) = self.ready_update_asset.clone() else {
            self.status = AutoUpdateStatus::Errored {
                message: "The downloaded update metadata was not found.".into(),
            };
            cx.notify();
            return;
        };
        let Some(downloaded_asset) = self.ready_downloaded_asset.clone() else {
            self.status = AutoUpdateStatus::Errored {
                message: "The downloaded update was not found.".into(),
            };
            cx.notify();
            return;
        };
        let Some(download_dir) = self.ready_download_dir.clone() else {
            self.status = AutoUpdateStatus::Errored {
                message: "The downloaded update directory was not found.".into(),
            };
            cx.notify();
            return;
        };

        self.status = AutoUpdateStatus::Installing {
            version: version.clone(),
        };
        cx.notify();

        #[cfg(target_os = "windows")]
        {
            let _ = (asset, download_dir, version);
            self.pending_windows_installer = Some(downloaded_asset);
            cx.quit();
        }

        #[cfg(not(target_os = "windows"))]
        {
            match install::install_asset(&asset, &downloaded_asset, &download_dir) {
                Ok(restart_path) => {
                    if let Some(path) = restart_path {
                        cx.set_restart_path(path);
                    }
                    cx.restart();
                }
                Err(error) => {
                    self.status = AutoUpdateStatus::Errored {
                        message: format!("{error:#}"),
                    };
                    cx.notify();
                }
            }
        }
    }
}

async fn poll_worker(
    rx: Receiver<WorkerMessage>,
    kind: UpdateCheckKind,
    this: WeakEntity<AutoUpdater>,
    app: &mut AsyncApp,
) {
    loop {
        loop {
            match rx.try_recv() {
                Ok(WorkerMessage::Status(status)) => {
                    if this
                        .update(app, |this, cx| this.apply_worker_status(status, cx))
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(WorkerMessage::Finished(result)) => {
                    let _ = this.update(app, |this, cx| {
                        this.apply_worker_finished(result, kind, cx);
                    });
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = this.update(app, |this, cx| {
                        this.apply_worker_finished(
                            Err("update worker disconnected unexpectedly".into()),
                            kind,
                            cx,
                        );
                    });
                    return;
                }
            }
        }

        app.background_executor().timer(WORKER_POLL_INTERVAL).await;
    }
}

fn run_update(
    current_version: Version,
    emit_status: impl Fn(AutoUpdateStatus),
) -> Result<UpdateOutcome> {
    if let Some(version) = dev_ready_version(&current_version)? {
        emit_status(AutoUpdateStatus::Downloading {
            version: version.clone(),
        });
        emit_status(AutoUpdateStatus::Installing {
            version: version.clone(),
        });
        return Ok(UpdateOutcome::ReadyToRestart {
            version,
            asset: None,
            downloaded_asset: None,
            download_dir: None,
        });
    }

    check_dependencies()?;

    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Monocurl/{CURRENT_VERSION}"))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("failed to create update HTTP client")?;

    let manifest = fetch_manifest(&client)?;
    let fetched_version = Version::parse(&manifest.version)
        .with_context(|| format!("invalid update version {:?}", manifest.version))?;
    if !version_is_newer(&current_version, &fetched_version) {
        return Ok(UpdateOutcome::NoUpdate {
            version: manifest.version,
        });
    }

    let asset = matching_asset(&manifest).cloned().with_context(|| {
        format!(
            "no update asset for {} {}",
            env::consts::OS,
            env::consts::ARCH
        )
    })?;

    validate_asset_kind(&asset)?;

    emit_status(AutoUpdateStatus::Downloading {
        version: manifest.version.clone(),
    });

    let download_dir = update_dir(&manifest.version)?;
    let downloaded_asset = download_asset(&client, &asset, &download_dir)?;
    verify_asset(&downloaded_asset, &asset.sha256)?;

    Ok(UpdateOutcome::ReadyToRestart {
        version: manifest.version,
        asset: Some(asset),
        downloaded_asset: Some(downloaded_asset),
        download_dir: Some(download_dir),
    })
}

fn dev_ready_version(current_version: &Version) -> Result<Option<String>> {
    if !cfg!(debug_assertions) {
        return Ok(None);
    }

    let Some(version) = env::var("MONOCURL_UPDATE_DEV_READY_VERSION")
        .ok()
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
    else {
        return Ok(None);
    };

    let parsed_version = Version::parse(&version)
        .with_context(|| format!("invalid MONOCURL_UPDATE_DEV_READY_VERSION {version:?}"))?;
    if !version_is_newer(current_version, &parsed_version) {
        bail!(
            "MONOCURL_UPDATE_DEV_READY_VERSION must be newer than v{}",
            current_version
        );
    }

    Ok(Some(version))
}

fn show_prompt(
    window: Option<AnyWindowHandle>,
    level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    cx: &mut App,
) {
    if let Some(window) = window {
        let _ = window.update(cx, |_, window, cx| {
            drop(window.prompt(level, message, detail, &["Ok"], cx));
        });
    }
}
