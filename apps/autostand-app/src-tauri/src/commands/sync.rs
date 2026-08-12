//! Cloud-sync folder detection for multi-device standup sync.
//!
//! On macOS and Windows the OS-native cloud provider (`iCloud Drive`,
//! `OneDrive`) exposes a well-known path the app can point `dailies_dir` at
//! for instant multi-device sync without shipping a sync engine. On Linux
//! there is no standard, so we probe the common self-hosted/proprietary
//! clients and let the user configure the path manually in Settings → Paths.
//!
//! Git sync (`git_ops::sync_pull` / `commit_push`) keeps running unchanged —
//! cloud sync is a second transport for instant availability, git remains the
//! authoritative history.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// One detected cloud-sync folder the user may point `dailies_dir` at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudFolder {
    /// Stable id, e.g. `icloud-drive`, `onedrive`, `syncthing`.
    pub id: String,
    /// Human-readable label, e.g. `iCloud Drive`.
    pub label: String,
    /// Absolute path to the folder root.
    pub path: String,
    /// Whether the folder exists on this machine.
    pub exists: bool,
    /// Cloud provider name, e.g. `iCloud`, `OneDrive`, `Syncthing`.
    pub provider: String,
}

fn home_join(segments: &[&str]) -> Option<PathBuf> {
    dirs::home_dir().map(|h| segments.iter().fold(h, |acc, s| acc.join(s)))
}

fn exists(path: &std::path::Path) -> bool {
    path.exists()
}

fn as_string(path: &std::path::Path) -> String {
    path.to_string_lossy().to_string()
}

/// Build a `CloudFolder`, probing the filesystem for `exists`.
fn folder(id: &str, label: &str, provider: &str, path: Option<PathBuf>) -> CloudFolder {
    match path {
        Some(p) => CloudFolder {
            id: id.to_string(),
            label: label.to_string(),
            path: as_string(&p),
            exists: exists(&p),
            provider: provider.to_string(),
        },
        None => CloudFolder {
            id: id.to_string(),
            label: label.to_string(),
            path: String::new(),
            exists: false,
            provider: provider.to_string(),
        },
    }
}

/// Scan `~/Library/CloudStorage/` for `OneDrive` / `Google Drive` / `Dropbox`
/// `FileProvider` mounts on macOS 13+.
fn macos_cloud_storage_mounts() -> Vec<CloudFolder> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let base = home.join("Library").join("CloudStorage");
        if base.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let path = entry.path();
                    let lower = name.to_lowercase();
                    let (id, label, provider) = if lower.starts_with("onedrive") {
                        (
                            "onedrive-cloudstorage",
                            "OneDrive (CloudStorage)",
                            "OneDrive",
                        )
                    } else if lower.starts_with("dropbox") {
                        ("dropbox-cloudstorage", "Dropbox (CloudStorage)", "Dropbox")
                    } else if lower.starts_with("google") || lower.contains("drive") {
                        (
                            "google-drive-cloudstorage",
                            "Google Drive (CloudStorage)",
                            "Google Drive",
                        )
                    } else if lower.starts_with("box") {
                        ("box-cloudstorage", "Box (CloudStorage)", "Box")
                    } else {
                        continue;
                    };
                    out.push(CloudFolder {
                        id: id.to_string(),
                        label: label.to_string(),
                        path: as_string(&path),
                        exists: true,
                        provider: provider.to_string(),
                    });
                }
            }
        }
    }
    out
}

/// Probe macOS `iCloud Drive` + `OneDrive` (`FileProvider`).
fn detect_macos() -> Vec<CloudFolder> {
    let mut out = Vec::new();
    // `iCloud Drive` root — the classic Mobile Documents path.
    out.push(folder(
        "icloud-drive",
        "iCloud Drive",
        "iCloud",
        home_join(&["Library", "Mobile Documents", "com~apple~CloudDocs"]),
    ));
    // App-private ubiquity container (syncs via `iCloud` but is not user-visible).
    if let Some(home) = dirs::home_dir() {
        let container = home
            .join("Library")
            .join("Mobile Documents")
            .join("com.miguel50flowers.autostand");
        out.push(folder(
            "icloud-app",
            "iCloud (app-private)",
            "iCloud",
            Some(container),
        ));
    }
    // Modern `FileProvider` mounts (`OneDrive`, `Dropbox`, `Google Drive`, `Box`…).
    out.extend(macos_cloud_storage_mounts());
    out
}

/// Probe Windows `OneDrive` (Known Folder Move or explicit folder).
fn detect_windows() -> Vec<CloudFolder> {
    let mut out = Vec::new();
    // `dirs::document_dir()` resolves to `OneDrive\Documents` when KFM is on.
    if let Some(docs) = dirs::document_dir() {
        out.push(folder(
            "onedrive-documents",
            "OneDrive / Documents",
            "OneDrive",
            Some(docs),
        ));
    }
    // `OneDrive` root (KFM off).
    if let Some(home) = dirs::home_dir() {
        out.push(folder(
            "onedrive",
            "OneDrive",
            "OneDrive",
            Some(home.join("OneDrive")),
        ));
    }
    out
}

/// Probe Linux self-hosted / third-party sync clients.
fn detect_linux() -> Vec<CloudFolder> {
    vec![
        folder("syncthing", "Syncthing", "Syncthing", home_join(&["Sync"])),
        folder(
            "nextcloud",
            "Nextcloud",
            "Nextcloud",
            home_join(&["Nextcloud"]),
        ),
        folder("dropbox", "Dropbox", "Dropbox", home_join(&["Dropbox"])),
    ]
}

/// Detect available cloud-sync folders on this machine.
///
/// Returns folders in preference order: platform-native first, then
/// well-known third-party clients. `exists = false` entries are included so
/// the UI can show "not detected" hints alongside detected ones.
#[tauri::command]
pub async fn detect_cloud_folders() -> Result<Vec<CloudFolder>, AppError> {
    let folders = if cfg!(target_os = "macos") {
        detect_macos()
    } else if cfg!(target_os = "windows") {
        detect_windows()
    } else {
        detect_linux()
    };
    Ok(folders)
}

#[cfg(test)]
mod tests {
    use super::{detect_linux, detect_macos, detect_windows, CloudFolder};

    #[test]
    fn macos_probes_icloud_drive_first() {
        let folders = detect_macos();
        assert!(folders.iter().any(|f| f.id == "icloud-drive"));
        // App-private ubiquity container is the second probe.
        assert!(folders.iter().any(|f| f.id == "icloud-app"));
    }

    #[test]
    fn macos_folders_have_a_provider_label() {
        for f in detect_macos() {
            assert!(!f.provider.is_empty(), "{:?} missing provider", f.id);
            assert!(!f.label.is_empty(), "{:?} missing label", f.id);
        }
    }

    #[test]
    fn windows_probes_onedrive_variants() {
        let folders = detect_windows();
        assert!(folders.iter().any(|f| f.id == "onedrive"));
        assert!(folders.iter().any(|f| f.id == "onedrive-documents"));
    }

    #[test]
    fn linux_probes_syncthing_nextcloud_dropbox() {
        let ids: Vec<String> = detect_linux().into_iter().map(|f| f.id).collect();
        assert!(ids.contains(&"syncthing".to_string()));
        assert!(ids.contains(&"nextcloud".to_string()));
        assert!(ids.contains(&"dropbox".to_string()));
    }

    #[test]
    fn cloud_folder_serializes_with_snake_case_fields() {
        let value = serde_json::to_value(CloudFolder {
            id: "icloud-drive".into(),
            label: "iCloud Drive".into(),
            path: "/tmp".into(),
            exists: true,
            provider: "iCloud".into(),
        })
        .unwrap();
        let obj = value.as_object().expect("CloudFolder is an object");
        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("label"));
        assert!(obj.contains_key("path"));
        assert!(obj.contains_key("exists"));
        assert!(obj.contains_key("provider"));
    }
}
