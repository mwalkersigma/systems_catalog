use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;

use crate::app::{
    AvailableUpdate, SystemsCatalogApp, UpdateApplyResult, UpdateCheckResult, UpdateCheckState,
};

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

impl SystemsCatalogApp {
    pub(super) fn maybe_start_lazy_update_check(&mut self) {
        if self.update_check_started {
            return;
        }

        let owner = self.update_repo_owner.trim();
        let repo = self.update_repo_name.trim();
        if owner.is_empty() || repo.is_empty() {
            self.update_check_state = UpdateCheckState::Disabled;
            self.update_check_started = true;
            return;
        }

        self.update_check_receiver = Some(spawn_update_check(owner.to_owned(), repo.to_owned()));
        self.update_check_state = UpdateCheckState::Checking;
        self.update_check_started = true;
    }

    pub(super) fn poll_update_tasks(&mut self) {
        if let Some(receiver) = &self.update_check_receiver {
            match receiver.try_recv() {
                Ok(Ok(Some(update))) => {
                    self.update_check_state = UpdateCheckState::UpdateAvailable(update);
                    self.status_message = "Update available".to_owned();
                    self.update_check_receiver = None;
                }
                Ok(Ok(None)) => {
                    self.update_check_state = UpdateCheckState::UpToDate;
                    self.update_check_receiver = None;
                }
                Ok(Err(error)) => {
                    self.update_check_state = UpdateCheckState::Error(error.to_string());
                    self.update_check_receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.update_check_state =
                        UpdateCheckState::Error("update check task disconnected".to_owned());
                    self.update_check_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if let Some(receiver) = &self.update_apply_receiver {
            match receiver.try_recv() {
                Ok(Ok(())) => {
                    self.update_check_state = UpdateCheckState::ReadyToRestart;
                    self.update_apply_receiver = None;
                    self.update_restart_requested = true;
                }
                Ok(Err(error)) => {
                    self.update_check_state = UpdateCheckState::Error(error.to_string());
                    self.update_apply_receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.update_check_state =
                        UpdateCheckState::Error("update install task disconnected".to_owned());
                    self.update_apply_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    pub(super) fn update_is_busy(&self) -> bool {
        self.update_check_receiver.is_some() || self.update_apply_receiver.is_some()
    }

    pub(super) fn update_restart_requested(&mut self) -> bool {
        if self.update_restart_requested {
            self.update_restart_requested = false;
            return true;
        }

        false
    }

    pub(super) fn confirm_and_start_update_install(&mut self) {
        let Some(update) = self.available_update().cloned() else {
            return;
        };

        if self.update_apply_receiver.is_some() {
            return;
        }

        let prompt = format!(
            "Version {} is available.\n\nInstall now and restart the app?",
            update.version
        );

        let answer = rfd::MessageDialog::new()
            .set_title("Systems Catalog Update")
            .set_description(prompt)
            .set_buttons(rfd::MessageButtons::YesNo)
            .set_level(rfd::MessageLevel::Info)
            .show();

        let accepted = {
            let normalized = format!("{answer:?}").to_ascii_lowercase();
            normalized == "true" || normalized.contains("yes") || normalized.contains("ok")
        };

        if !accepted {
            self.status_message = "Update cancelled".to_owned();
            return;
        }

        self.status_message = format!("Downloading update {}...", update.version);
        self.update_check_state = UpdateCheckState::Applying;
        self.update_apply_receiver = Some(spawn_update_install(update));
    }

    pub(super) fn open_update_release_page(&mut self) {
        let release_url = self
            .available_update()
            .map(|update| update.release_url.clone())
            .unwrap_or_default();

        if release_url.trim().is_empty() {
            return;
        }

        match webbrowser::open(release_url.as_str()) {
            Ok(_) => {
                self.status_message = "Opened release page".to_owned();
            }
            Err(error) => {
                self.status_message = format!("Could not open release page: {error}");
            }
        }
    }

    pub(super) fn available_update(&self) -> Option<&AvailableUpdate> {
        if let UpdateCheckState::UpdateAvailable(update) = &self.update_check_state {
            return Some(update);
        }

        None
    }
}

fn spawn_update_check(owner: String, repo: String) -> mpsc::Receiver<UpdateCheckResult> {
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let result = fetch_available_update(owner.as_str(), repo.as_str());
        let _ = sender.send(result);
    });

    receiver
}

fn fetch_available_update(owner: &str, repo: &str) -> UpdateCheckResult {
    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");

    let client = Client::builder().build()?;
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            format!("systems_catalog/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .context("failed to query latest release")?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "update check failed with status {}",
            response.status()
        ));
    }

    let latest_release: GithubRelease = response.json().context("failed to parse release data")?;
    let parsed_latest = Version::parse(latest_release.tag_name.trim_start_matches('v'))
        .with_context(|| format!("invalid release tag {}", latest_release.tag_name))?;

    if parsed_latest <= current_version {
        return Ok(None);
    }

    let preferred_asset_name = preferred_asset_name();
    let selected_asset = preferred_asset_name
        .as_deref()
        .and_then(|name| {
            latest_release
                .assets
                .iter()
                .find(|asset| asset.name == name)
        })
        .or_else(|| latest_release.assets.first());

    let update = AvailableUpdate {
        version: parsed_latest.to_string(),
        tag_name: latest_release.tag_name,
        release_url: latest_release.html_url,
        asset_name: selected_asset.map(|asset| asset.name.clone()),
        download_url: selected_asset.map(|asset| asset.browser_download_url.clone()),
    };

    Ok(Some(update))
}

fn preferred_asset_name() -> Option<String> {
    match (env::consts::OS, env::consts::ARCH) {
        ("windows", "x86_64") => Some("systems_catalog-windows-x86_64.exe".to_owned()),
        ("linux", "x86_64") => Some("systems_catalog-linux-x86_64".to_owned()),
        ("macos", "aarch64") => Some("systems_catalog-macos-aarch64".to_owned()),
        ("macos", "x86_64") => Some("systems_catalog-macos-x86_64".to_owned()),
        _ => None,
    }
}

fn spawn_update_install(update: AvailableUpdate) -> mpsc::Receiver<UpdateApplyResult> {
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let result = install_update(update);
        let _ = sender.send(result);
    });

    receiver
}

fn install_update(update: AvailableUpdate) -> UpdateApplyResult {
    let download_url = update
        .download_url
        .as_deref()
        .ok_or_else(|| anyhow!("no release asset available for this platform"))?;

    let exe_path = env::current_exe().context("failed to locate current executable")?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow!("current executable has no parent directory"))?;

    let staged_binary_path = staged_binary_path(exe_dir, &exe_path, update.asset_name.as_deref());
    download_to_path(download_url, staged_binary_path.as_path())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            staged_binary_path.as_path(),
            fs::Permissions::from_mode(0o755),
        )
        .context("failed to mark staged binary executable")?;
    }

    schedule_binary_swap_and_restart(exe_path.as_path(), staged_binary_path.as_path())?;

    Ok(())
}

fn staged_binary_path(exe_dir: &Path, exe_path: &Path, asset_name: Option<&str>) -> PathBuf {
    if let Some(asset) = asset_name {
        return exe_dir.join(format!("{asset}.new"));
    }

    let fallback_name = exe_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "systems_catalog".to_owned());

    exe_dir.join(format!("{fallback_name}.new"))
}

fn download_to_path(download_url: &str, target_path: &Path) -> Result<()> {
    let client = Client::builder().build()?;
    let response = client
        .get(download_url)
        .header(
            "User-Agent",
            format!("systems_catalog/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .with_context(|| format!("failed to download update asset from {download_url}"))?;

    if !response.status().is_success() {
        return Err(anyhow!("download failed with status {}", response.status()));
    }

    let content = response
        .bytes()
        .context("failed to read downloaded bytes")?;
    fs::write(target_path, &content)
        .with_context(|| format!("failed to write staged update at {}", target_path.display()))?;

    Ok(())
}

fn schedule_binary_swap_and_restart(target_exe: &Path, staged_binary: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return schedule_windows_swap(target_exe, staged_binary);
    }

    #[cfg(not(target_os = "windows"))]
    {
        schedule_unix_swap(target_exe, staged_binary)
    }
}

#[cfg(target_os = "windows")]
fn schedule_windows_swap(target_exe: &Path, staged_binary: &Path) -> Result<()> {
    let script_path = temp_script_path("systems_catalog_apply_update", "ps1");
    let target = ps_quote(target_exe.to_string_lossy().as_ref());
    let staged = ps_quote(staged_binary.to_string_lossy().as_ref());
    let script_self = ps_quote(script_path.to_string_lossy().as_ref());

    let script_body = format!(
        "$target = {target}\n$staged = {staged}\n$scriptSelf = {script_self}\n\
         for ($i = 0; $i -lt 80; $i++) {{\n\
         \ttry {{ Move-Item -Force $staged $target; break }} catch {{ Start-Sleep -Milliseconds 500 }}\n\
         }}\n\
         Start-Process -FilePath $target\n\
         Remove-Item -Force $scriptSelf -ErrorAction SilentlyContinue\n"
    );

    fs::write(script_path.as_path(), script_body)
        .context("failed to write windows updater script")?;

    Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(script_path.as_os_str())
        .spawn()
        .context("failed to launch updater script")?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(target_os = "windows"))]
fn schedule_unix_swap(target_exe: &Path, staged_binary: &Path) -> Result<()> {
    let script_path = temp_script_path("systems_catalog_apply_update", "sh");
    let script_body = format!(
        "#!/usr/bin/env sh\nset -eu\nTARGET=\"{}\"\nSTAGED=\"{}\"\nfor i in $(seq 1 80); do\n  if mv -f \"$STAGED\" \"$TARGET\" 2>/dev/null; then\n    break\n  fi\n  sleep 0.5\ndone\nchmod +x \"$TARGET\"\n\"$TARGET\" &\nrm -f \"{}\"\n",
        target_exe.display(),
        staged_binary.display(),
        script_path.display()
    );

    fs::write(script_path.as_path(), script_body).context("failed to write unix updater script")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(script_path.as_path(), fs::Permissions::from_mode(0o755))
            .context("failed to mark updater script executable")?;
    }

    Command::new("sh")
        .arg(script_path.as_os_str())
        .spawn()
        .context("failed to launch updater script")?;

    Ok(())
}

fn temp_script_path(prefix: &str, extension: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    env::temp_dir().join(format!("{prefix}_{timestamp}.{extension}"))
}
