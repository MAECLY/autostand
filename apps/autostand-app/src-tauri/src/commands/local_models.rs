//! Built-in local-model catalog and lifecycle commands.
//!
//! Model weights are user-initiated downloads stored below the platform app-data
//! directory. Downloads are resumable and become visible to inference only after
//! their pinned SHA-256 has been verified and the partial file has been synced.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use autostand_runlog::proc::{run_process, ProcSpec, StreamPolicy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use crate::error::AppError;
use crate::run_log::{Run, RunKind};

const LOCAL_PROVIDER_ID: &str = "builtin-local";

/// Budget for one process-table read or one signal.
///
/// Previously unbounded: a `ps` blocked on an unresponsive filesystem would have
/// hung "Unload" with no way back.
const PROCESS_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Event emitted while a model is downloaded or verified.
pub const LOCAL_MODEL_PROGRESS_EVENT: &str = "local-model-progress";

/// One catalog item as exposed over Tauri IPC.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalModelInfo {
    pub id: String,
    pub display_name: String,
    pub tier: String,
    pub quality: String,
    pub format: String,
    pub size_bytes: u64,
    pub context_length: u32,
    pub status: LocalModelStatus,
    pub selected: bool,
    pub license: String,
    pub license_url: String,
    pub terms_required: bool,
    pub downloaded_bytes: u64,
    /// Size of this model's reusable llama.cpp prompt/KV cache, `0` when cold.
    pub runtime_cache_bytes: u64,
    pub error: Option<String>,
}

/// What [`unload_local_models`] actually released.
///
/// Every field is a real, observable effect: no field reports work the local
/// architecture cannot perform.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct LocalRuntimeUnload {
    pub processes_terminated: u32,
    pub caches_removed: u32,
    pub bytes_freed: u64,
}

/// Durable model lifecycle state derived from files on disk.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalModelStatus {
    NotDownloaded,
    Downloading,
    Available,
    Corrupted,
    Error,
}

/// Progress event payload. `downloaded_bytes == total_bytes` means verification
/// completed and the final file is available.
#[derive(Debug, Clone, Serialize)]
pub struct LocalModelProgress {
    pub model_id: String,
    pub status: LocalModelStatus,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_second: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct TermsAcceptance {
    accepted: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ModelState {
    selected_model: Option<String>,
    terms: TermsAcceptance,
}

#[derive(Debug, Clone, Copy)]
struct ModelDefinition {
    id: &'static str,
    display_name: &'static str,
    tier: &'static str,
    quality: &'static str,
    file_name: &'static str,
    url: &'static str,
    size_bytes: u64,
    sha256: &'static str,
    context_length: u32,
    license: &'static str,
    license_url: &'static str,
    terms_version: Option<&'static str>,
}

const CATALOG: &[ModelDefinition] = &[
    ModelDefinition {
        id: "gemma3:1b",
        display_name: "Gemma 3 1B (Fast)",
        tier: "extra_small",
        quality: "fast",
        file_name: "gemma-3-1b-it-Q8_0.gguf",
        url: "https://huggingface.co/bartowski/google_gemma-3-1b-it-GGUF/resolve/116f76234503685a98f572982177b11d44ec8ff1/google_gemma-3-1b-it-Q8_0.gguf",
        size_bytes: 1_069_306_624,
        sha256: "375e12a4a18929a641f9744b060d4a7cf4e279530750555828ec0c117870bc96",
        context_length: 32_768,
        license: "Gemma Terms of Use",
        license_url: "https://ai.google.dev/gemma/terms",
        terms_version: Some("2025-03-25"),
    },
    ModelDefinition {
        id: "qwen3.5:2b",
        display_name: "Qwen 3.5 2B (Balanced)",
        tier: "small",
        quality: "balanced",
        file_name: "Qwen3.5-2B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/f6d5376be1edb4d416d56da11e5397a961aca8ae2/Qwen3.5-2B-Q4_K_M.gguf",
        size_bytes: 1_280_835_840,
        sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
        context_length: 32_768,
        license: "Apache-2.0",
        license_url: "https://www.apache.org/licenses/LICENSE-2.0",
        terms_version: None,
    },
    ModelDefinition {
        id: "gemma3:4b",
        display_name: "Gemma 3 4B (Balanced)",
        tier: "medium",
        quality: "balanced",
        file_name: "gemma-3-4b-it-Q4_K_M.gguf",
        url: "https://huggingface.co/bartowski/google_gemma-3-4b-it-GGUF/resolve/71506238f970075ca85125cd749c28b1b0eee84e4/google_gemma-3-4b-it-Q4_K_M.gguf",
        size_bytes: 2_489_758_112,
        sha256: "4996030242583a40aa151ff93f49ed787ac8c25e4120c3ae4588b2e2a7d1ae94",
        context_length: 32_768,
        license: "Gemma Terms of Use",
        license_url: "https://ai.google.dev/gemma/terms",
        terms_version: Some("2025-03-25"),
    },
    ModelDefinition {
        id: "qwen3.5:4b",
        display_name: "Qwen 3.5 4B (High Quality)",
        tier: "large",
        quality: "high_quality",
        file_name: "Qwen3.5-4B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/e87f176479d0855a907a41277aca2f8ee7a095233/Qwen3.5-4B-Q4_K_M.gguf",
        size_bytes: 2_740_937_888,
        sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        context_length: 32_768,
        license: "Apache-2.0",
        license_url: "https://www.apache.org/licenses/LICENSE-2.0",
        terms_version: None,
    },
];

static CANCELLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn cancellations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn definition(model_id: &str) -> Result<&'static ModelDefinition, AppError> {
    CATALOG
        .iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| AppError::Invalid(format!("unknown local model: {model_id}")))
}

fn models_dir(_app: &AppHandle) -> PathBuf {
    crate::commands::state_dir().join("models").join("local")
}

/// Installed model ids exposed to the provider model picker.
pub(crate) fn available_model_ids(app: &AppHandle) -> Vec<String> {
    let dir = models_dir(app);
    CATALOG
        .iter()
        .filter(|model| status_for(&dir, model).0 == LocalModelStatus::Available)
        .map(|model| model.id.to_string())
        .collect()
}

/// Return the model selected by the managed local-model catalogue.
pub(crate) fn selected_model_id(app: &AppHandle) -> Option<String> {
    read_selected(&models_dir(app))
}

fn activate_local_provider(config: &mut crate::commands::types::AppConfig, model_id: &str) {
    use crate::commands::types::{ProviderConfig, ProviderMode};

    if let Some(provider) = config
        .llm
        .providers
        .iter_mut()
        .find(|provider| provider.id == LOCAL_PROVIDER_ID)
    {
        provider.enabled = true;
        provider.mode = ProviderMode::CliOnly;
        provider.model = model_id.to_string();
        provider.timeout_secs = provider.timeout_secs.max(300);
    } else {
        config.llm.providers.push(ProviderConfig {
            id: LOCAL_PROVIDER_ID.to_string(),
            enabled: true,
            mode: ProviderMode::CliOnly,
            model: model_id.to_string(),
            timeout_secs: 300,
            ..ProviderConfig::default()
        });
    }
    config.llm.preferred_provider = LOCAL_PROVIDER_ID.to_string();
    config
        .llm
        .provider_order
        .retain(|provider| provider != LOCAL_PROVIDER_ID);
    config
        .llm
        .provider_order
        .insert(0, LOCAL_PROVIDER_ID.to_string());
}

fn deactivate_local_provider(config: &mut crate::commands::types::AppConfig, model_id: &str) {
    if let Some(provider) = config
        .llm
        .providers
        .iter_mut()
        .find(|provider| provider.id == LOCAL_PROVIDER_ID)
    {
        if provider.model == model_id {
            provider.enabled = false;
            provider.model.clear();
        }
    }
    if config.llm.preferred_provider == LOCAL_PROVIDER_ID {
        config.llm.preferred_provider = config
            .llm
            .provider_order
            .iter()
            .find(|provider| provider.as_str() != LOCAL_PROVIDER_ID)
            .cloned()
            .unwrap_or_default();
    }
    config
        .llm
        .provider_order
        .retain(|provider| provider != LOCAL_PROVIDER_ID);
}

fn selected_path(models_dir: &Path) -> PathBuf {
    models_dir.join("local-models.json")
}

fn terms_path(models_dir: &Path) -> PathBuf {
    models_dir.join("local-models.json")
}

fn read_selected(models_dir: &Path) -> Option<String> {
    read_model_state(models_dir).selected_model
}

fn read_model_state(models_dir: &Path) -> ModelState {
    std::fs::read(terms_path(models_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn model_paths(models_dir: &Path, model: &ModelDefinition) -> (PathBuf, PathBuf) {
    let final_path = models_dir.join(model.file_name);
    let partial_path = models_dir.join(format!("{}.part", model.file_name));
    (final_path, partial_path)
}

fn error_path(models_dir: &Path, model: &ModelDefinition) -> PathBuf {
    models_dir.join(format!("{}.error", model.file_name))
}

/// Directory holding the llama.cpp prompt/KV caches written under `keep_ready`.
///
/// Mirrors `autostand_adapters::llm::builtin_local::prompt_cache_path`, which
/// resolves the same location from the adapter side.
fn runtime_cache_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("runtime-cache")
}

fn runtime_cache_path(models_dir: &Path, model: &ModelDefinition) -> PathBuf {
    runtime_cache_dir(models_dir).join(format!("{}.prompt-cache", model.file_name))
}

fn runtime_cache_bytes(models_dir: &Path, model: &ModelDefinition) -> u64 {
    runtime_cache_path(models_dir, model)
        .metadata()
        .map_or(0, |metadata| metadata.len())
}

fn status_for(
    models_dir: &Path,
    model: &ModelDefinition,
) -> (LocalModelStatus, u64, Option<String>) {
    let (final_path, partial_path) = model_paths(models_dir, model);
    if let Ok(metadata) = final_path.metadata() {
        return if metadata.len() == model.size_bytes {
            (LocalModelStatus::Available, metadata.len(), None)
        } else {
            (
                LocalModelStatus::Corrupted,
                metadata.len(),
                Some("downloaded file has an unexpected size".to_string()),
            )
        };
    }
    if let Ok(message) = std::fs::read_to_string(error_path(models_dir, model)) {
        return (
            LocalModelStatus::Error,
            partial_path.metadata().map_or(0, |metadata| metadata.len()),
            Some(message),
        );
    }
    if let Ok(metadata) = partial_path.metadata() {
        return (LocalModelStatus::Downloading, metadata.len(), None);
    }
    (LocalModelStatus::NotDownloaded, 0, None)
}

/// List curated models and their derived on-disk status.
#[tauri::command]
pub async fn list_local_models(app_handle: AppHandle) -> Result<Vec<LocalModelInfo>, AppError> {
    let dir = models_dir(&app_handle);
    let selected = read_selected(&dir);
    let state = read_model_state(&dir);
    Ok(CATALOG
        .iter()
        .map(|model| {
            let (status, downloaded_bytes, error) = status_for(&dir, model);
            LocalModelInfo {
                id: model.id.to_string(),
                display_name: model.display_name.to_string(),
                tier: model.tier.to_string(),
                quality: model.quality.to_string(),
                format: "GGUF".to_string(),
                size_bytes: model.size_bytes,
                context_length: model.context_length,
                status,
                selected: selected.as_deref() == Some(model.id),
                license: model.license.to_string(),
                license_url: model.license_url.to_string(),
                terms_required: model.terms_version.is_some_and(|version| {
                    state.terms.accepted.get(model.id).map(String::as_str) != Some(version)
                }),
                downloaded_bytes,
                runtime_cache_bytes: runtime_cache_bytes(&dir, model),
                error,
            }
        })
        .collect())
}

/// Record acceptance of the exact catalog terms version for a gated model.
#[tauri::command]
pub async fn accept_local_model_terms(
    app_handle: AppHandle,
    model_id: String,
) -> Result<(), AppError> {
    let model = definition(&model_id)?;
    let Some(version) = model.terms_version else {
        return Ok(());
    };
    let dir = models_dir(&app_handle);
    std::fs::create_dir_all(&dir)?;
    let path = terms_path(&dir);
    let mut state = read_model_state(&dir);
    state
        .terms
        .accepted
        .insert(model.id.to_string(), version.to_string());
    atomic_write_json(&path, &state)
}

fn ensure_terms_accepted(models_dir: &Path, model: &ModelDefinition) -> Result<(), AppError> {
    let Some(version) = model.terms_version else {
        return Ok(());
    };
    let state = read_model_state(models_dir);
    if state.terms.accepted.get(model.id).map(String::as_str) == Some(version) {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "accept {} before downloading {}",
            model.license, model.display_name
        )))
    }
}

/// Download or resume a model, verify its pinned hash, then atomically install it.
#[tauri::command]
pub async fn download_local_model(app_handle: AppHandle, model_id: String) -> Result<(), AppError> {
    let model = *definition(&model_id)?;
    let dir = models_dir(&app_handle);
    std::fs::create_dir_all(&dir)?;
    ensure_terms_accepted(&dir, &model)?;
    let _ = std::fs::remove_file(error_path(&dir, &model));

    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = cancellations()
            .lock()
            .map_err(|_| AppError::Io("local model download state is poisoned".to_string()))?;
        if active.contains_key(model.id) {
            return Err(AppError::Invalid(format!(
                "download already running: {}",
                model.id
            )));
        }
        active.insert(model.id.to_string(), Arc::clone(&cancelled));
    }

    let result = download_model(&app_handle, &dir, model, &cancelled).await;
    if let Err(error) = &result {
        if !cancelled.load(Ordering::Relaxed) {
            let message = error.to_string();
            let _ = atomic_write_bytes(&error_path(&dir, &model), message.as_bytes());
            let downloaded = model_paths(&dir, &model)
                .1
                .metadata()
                .map_or(0, |metadata| metadata.len());
            emit_progress(
                &app_handle,
                model,
                LocalModelStatus::Error,
                downloaded,
                0,
                Some(message),
            );
        }
    }
    if let Ok(mut active) = cancellations().lock() {
        active.remove(model.id);
    }
    if let Ok(config) = crate::commands::load_config(&app_handle) {
        let notification = crate::notifications::SystemNotification::local_model_download(
            model.display_name,
            result.is_ok(),
        );
        if let Err(error) =
            crate::notifications::notify_gui(&app_handle, &config.notifications, &notification)
                .await
        {
            tracing::warn!(model = model.id, %error, "could not deliver local-model notification");
        }
    }
    result
}

#[allow(clippy::too_many_lines)]
async fn download_model(
    app: &AppHandle,
    dir: &Path,
    model: ModelDefinition,
    cancelled: &AtomicBool,
) -> Result<(), AppError> {
    let (final_path, partial_path) = model_paths(dir, &model);
    if final_path
        .metadata()
        .is_ok_and(|metadata| metadata.len() == model.size_bytes)
    {
        return Ok(());
    }
    let resume_from = partial_path.metadata().map_or(0, |metadata| metadata.len());
    if resume_from > model.size_bytes {
        std::fs::remove_file(&partial_path)?;
    }
    let resume_from = partial_path.metadata().map_or(0, |metadata| metadata.len());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30 * 60))
        .build()
        .map_err(|error| AppError::Io(format!("build download client: {error}")))?;
    let mut request = client.get(model.url);
    if resume_from > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let mut response = request
        .send()
        .await
        .map_err(|error| AppError::Io(format!("download {}: {error}", model.id)))?;
    if !response.status().is_success() {
        return Err(AppError::Io(format!(
            "download {} failed with HTTP {}",
            model.id,
            response.status().as_u16()
        )));
    }
    let append = resume_from > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!append)
        .append(append)
        .open(&partial_path)?;
    let mut downloaded = if append { resume_from } else { 0 };
    let started = Instant::now();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| AppError::Io(format!("read {} download: {error}", model.id)))?
    {
        if cancelled.load(Ordering::Relaxed) {
            emit_progress(
                app,
                model,
                LocalModelStatus::Downloading,
                downloaded,
                0,
                None,
            );
            return Err(AppError::Io(format!("download cancelled: {}", model.id)));
        }
        file.write_all(&chunk)?;
        downloaded = downloaded.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        let elapsed = started.elapsed().as_secs().max(1);
        emit_progress(
            app,
            model,
            LocalModelStatus::Downloading,
            downloaded,
            downloaded.saturating_sub(resume_from) / elapsed,
            None,
        );
    }
    file.sync_all()?;
    drop(file);
    if downloaded != model.size_bytes {
        let message = format!(
            "downloaded size mismatch: expected {}, got {downloaded}",
            model.size_bytes
        );
        emit_progress(
            app,
            model,
            LocalModelStatus::Corrupted,
            downloaded,
            0,
            Some(message.clone()),
        );
        return Err(AppError::Io(message));
    }
    let hash_path = partial_path.clone();
    let actual_hash = tokio::task::spawn_blocking(move || sha256_file(&hash_path))
        .await
        .map_err(|error| AppError::Io(format!("verify task failed: {error}")))??;
    if actual_hash != model.sha256 {
        let message = "downloaded model failed SHA-256 verification".to_string();
        emit_progress(
            app,
            model,
            LocalModelStatus::Corrupted,
            downloaded,
            0,
            Some(message.clone()),
        );
        return Err(AppError::Io(message));
    }
    std::fs::rename(&partial_path, &final_path)?;
    let _ = std::fs::remove_file(error_path(dir, &model));
    sync_directory(dir)?;
    emit_progress(app, model, LocalModelStatus::Available, downloaded, 0, None);
    Ok(())
}

fn emit_progress(
    app: &AppHandle,
    model: ModelDefinition,
    status: LocalModelStatus,
    downloaded_bytes: u64,
    bytes_per_second: u64,
    error: Option<String>,
) {
    let _ = app.emit(
        LOCAL_MODEL_PROGRESS_EVENT,
        LocalModelProgress {
            model_id: model.id.to_string(),
            status,
            downloaded_bytes,
            total_bytes: model.size_bytes,
            bytes_per_second,
            error,
        },
    );
}

/// Request cancellation. The partial file is deliberately retained for resume.
#[tauri::command]
pub async fn cancel_local_model_download(model_id: String) -> Result<bool, AppError> {
    definition(&model_id)?;
    let active = cancellations()
        .lock()
        .map_err(|_| AppError::Io("local model download state is poisoned".to_string()))?;
    Ok(active.get(&model_id).is_some_and(|flag| {
        flag.store(true, Ordering::Relaxed);
        true
    }))
}

/// Delete final and partial files for a model.
#[tauri::command]
pub async fn delete_local_model(app_handle: AppHandle, model_id: String) -> Result<(), AppError> {
    let model = definition(&model_id)?;
    let _ = cancel_local_model_download(model_id.clone()).await?;
    let dir = models_dir(&app_handle);
    let was_selected = read_selected(&dir).as_deref() == Some(model.id);
    if was_selected {
        let mut config = crate::commands::load_config(&app_handle)?;
        deactivate_local_provider(&mut config, model.id);
        crate::commands::save_config(&app_handle, &config)?;
    }
    let (final_path, partial_path) = model_paths(&dir, model);
    let runtime_cache = runtime_cache_path(&dir, model);
    for path in [
        final_path,
        partial_path,
        error_path(&dir, model),
        runtime_cache,
    ] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if was_selected {
        let mut state = read_model_state(&dir);
        state.selected_model = None;
        atomic_write_json(&selected_path(&dir), &state)?;
    }
    Ok(())
}

/// Select an installed, size-valid model as the built-in provider model.
#[tauri::command]
pub async fn select_local_model(app_handle: AppHandle, model_id: String) -> Result<(), AppError> {
    let model = definition(&model_id)?;
    let dir = models_dir(&app_handle);
    let (status, _, _) = status_for(&dir, model);
    if status != LocalModelStatus::Available {
        return Err(AppError::Invalid(format!(
            "local model is not available: {model_id}"
        )));
    }
    std::fs::create_dir_all(&dir)?;
    let mut state = read_model_state(&dir);
    let previous_state = state.clone();
    state.selected_model = Some(model.id.to_string());
    atomic_write_json(&selected_path(&dir), &state)?;

    let config_result = (|| {
        let mut config = crate::commands::load_config(&app_handle)?;
        activate_local_provider(&mut config, model.id);
        crate::commands::save_config(&app_handle, &config)
    })();
    if let Err(error) = config_result {
        let _ = atomic_write_json(&selected_path(&dir), &previous_state);
        return Err(error);
    }
    Ok(())
}

/// Release everything the built-in local runtime can still be holding.
///
/// WHY this is not a no-op even though inference is one-shot: nothing stays
/// resident *by design*, but two things survive a render anyway.
/// 1. `keep_ready` hands llama.cpp a per-model `--prompt-cache` file that is
///    written on every run and never expires — that file, not RAM, is what
///    keeps the model "warm", and it is several hundred MiB per model.
/// 2. The adapter kills the sidecar on timeout, but the sidecar's own
///    `llama-completion`/`llama-cli` grandchild is not in that process group and
///    survives with the full GGUF mapped into memory.
///
/// So: terminate every process that still references the managed models
/// directory, then delete the caches. The returned summary reports only what
/// really happened.
#[tauri::command]
pub async fn unload_local_models(app_handle: AppHandle) -> Result<LocalRuntimeUnload, AppError> {
    let dir = models_dir(&app_handle);
    let run = Run::open(&app_handle, RunKind::LocalRuntime);
    let outcome = run.scope(unload_runtime(&dir)).await;
    let summary = outcome.as_ref().map_or_else(
        |_| String::new(),
        |unload| {
            format!(
                "{} process(es) stopped, {} cache file(s) removed",
                unload.processes_terminated, unload.caches_removed
            )
        },
    );
    run.settle(outcome, &summary)
}

/// Processes first: a live run would rewrite the prompt cache as it exits.
///
/// Was a `spawn_blocking` closure. It could not stay one: a blocking task does
/// not inherit the run's sink, so everything it spawned was invisible. The work
/// is now async — the process probes are the only slow part and they are
/// subprocesses anyway.
async fn unload_runtime(models_dir: &Path) -> Result<LocalRuntimeUnload, AppError> {
    let processes_terminated = terminate_model_processes(models_dir).await;
    let (caches_removed, bytes_freed) = purge_runtime_caches(models_dir)?;
    Ok(LocalRuntimeUnload {
        processes_terminated,
        caches_removed,
        bytes_freed,
    })
}

fn purge_runtime_caches(models_dir: &Path) -> Result<(u32, u64), AppError> {
    let entries = match std::fs::read_dir(runtime_cache_dir(models_dir)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(error) => return Err(error.into()),
    };
    let mut removed = 0_u32;
    let mut bytes = 0_u64;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let size = entry.metadata().map_or(0, |metadata| metadata.len());
        match std::fs::remove_file(entry.path()) {
            Ok(()) => {
                removed = removed.saturating_add(1);
                bytes = bytes.saturating_add(size);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok((removed, bytes))
}

async fn terminate_model_processes(models_dir: &Path) -> u32 {
    let Some(needle) = models_dir.to_str() else {
        return 0;
    };
    let Some(table) = process_table().await else {
        return 0;
    };
    let mut terminated = 0_usize;
    for pid in model_holding_pids(&table, needle, std::process::id()) {
        if terminate(pid).await {
            terminated += 1;
        } else {
            tracing::warn!(pid, "could not terminate a local runtime process");
        }
    }
    u32::try_from(terminated).unwrap_or(u32::MAX)
}

/// PIDs from a `pid<space>command-line` table whose command line references
/// `models_dir`.
///
/// WHY the models directory instead of a binary name: llama.cpp is started as
/// `llama-completion --model <models_dir>/<file>.gguf`, so the path is the only
/// signal that separates *our* runtime from a `llama-cli` the user launched
/// themselves — which this command must never kill.
fn model_holding_pids(table: &str, models_dir: &str, self_pid: u32) -> Vec<u32> {
    if models_dir.is_empty() {
        return Vec::new();
    }
    table
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim_start().split_once(char::is_whitespace)?;
            let pid = pid.parse::<u32>().ok()?;
            (pid != self_pid && command.contains(models_dir)).then_some(pid)
        })
        .collect()
}

/// Snapshot of `pid` + full command line for every process, or `None` when the
/// platform would not answer.
///
/// WHY a subprocess: the workspace pulls in no `libc`/`sysinfo` dependency, the
/// same reason `autostand-scheduler::lock` probes liveness with `kill -0`.
///
/// [`StreamPolicy::Silent`] and not negotiable: this stdout is **every command
/// line on the machine**, which is the single most sensitive thing the app ever
/// reads. The run's closing line reports how many processes were stopped.
#[cfg(unix)]
async fn process_table() -> Option<String> {
    let output = run_process(
        ProcSpec::new("ps", RunKind::LocalRuntime.step())
            .args(["-A", "-o", "pid=,args="])
            .timeout(PROCESS_PROBE_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .ok()?;
    output.success.then_some(output.stdout)
}

/// Windows variant. `tasklist` cannot print command lines, and `wmic` is
/// deprecated, so the CIM provider is the only source of the model path.
#[cfg(windows)]
async fn process_table() -> Option<String> {
    let output = run_process(
        ProcSpec::new("powershell", RunKind::LocalRuntime.step())
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId) $($_.CommandLine)\" }",
            ])
            .timeout(PROCESS_PROBE_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .ok()?;
    output.success.then_some(output.stdout)
}

/// [`StreamPolicy::Silent`]: one spawn per matched process, and the pid is not
/// something a user can act on. The run reports the total instead.
#[cfg(unix)]
async fn terminate(pid: u32) -> bool {
    run_process(
        ProcSpec::new("kill", RunKind::LocalRuntime.step())
            .args(["-TERM", &pid.to_string()])
            .timeout(PROCESS_PROBE_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .is_ok_and(|output| output.success)
}

#[cfg(windows)]
async fn terminate(pid: u32) -> bool {
    run_process(
        ProcSpec::new("taskkill", RunKind::LocalRuntime.step())
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .timeout(PROCESS_PROBE_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .is_ok_and(|output| output.success)
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024].into_boxed_slice();
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    atomic_write_bytes(path, &bytes)
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io("state file has no parent directory".to_string()))?;
    std::fs::create_dir_all(parent)?;
    let temp = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temp, path)?;
    sync_directory(parent)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), AppError> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_four_unique_pinned_models() {
        assert_eq!(CATALOG.len(), 4);
        let mut ids = CATALOG.iter().map(|model| model.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 4);
        for model in CATALOG {
            assert_eq!(model.sha256.len(), 64);
            assert!(model.url.contains("/resolve/"));
            assert!(!model.url.contains("/resolve/main/"));
            assert!(model.size_bytes > 1_000_000_000);
        }
    }

    #[test]
    fn status_distinguishes_partial_available_and_corrupt() {
        let temp = tempfile::tempdir().unwrap();
        let model = &CATALOG[0];
        assert_eq!(
            status_for(temp.path(), model).0,
            LocalModelStatus::NotDownloaded
        );
        let (final_path, partial_path) = model_paths(temp.path(), model);
        std::fs::write(&partial_path, b"partial").unwrap();
        assert_eq!(
            status_for(temp.path(), model).0,
            LocalModelStatus::Downloading
        );
        std::fs::remove_file(partial_path).unwrap();
        std::fs::write(&final_path, b"bad").unwrap();
        assert_eq!(
            status_for(temp.path(), model).0,
            LocalModelStatus::Corrupted
        );
    }

    #[test]
    fn sha256_is_computed_incrementally() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("payload");
        std::fs::write(&path, b"autostand").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "56576f1ec71f3dfa63a18fd020d6adfc11ae83c61114dbdf442161c8102fa9bd"
        );
    }

    #[test]
    fn selecting_a_model_activates_and_prioritizes_the_local_provider() {
        let mut config = crate::commands::types::AppConfig::default();
        config.llm.preferred_provider = "claude".to_string();
        config.llm.provider_order = vec!["claude".to_string(), "openai".to_string()];

        activate_local_provider(&mut config, "qwen3.5:2b");

        assert_eq!(config.llm.preferred_provider, LOCAL_PROVIDER_ID);
        assert_eq!(config.llm.provider_order[0], LOCAL_PROVIDER_ID);
        let provider = config
            .llm
            .providers
            .iter()
            .find(|provider| provider.id == LOCAL_PROVIDER_ID)
            .unwrap();
        assert!(provider.enabled);
        assert_eq!(provider.model, "qwen3.5:2b");
        assert_eq!(provider.mode, crate::commands::types::ProviderMode::CliOnly);
    }

    /// Seed `runtime-cache/<file>.prompt-cache` for `model` with `bytes` bytes.
    fn seed_cache(models_dir: &Path, model: &ModelDefinition, bytes: usize) -> PathBuf {
        let path = runtime_cache_path(models_dir, model);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![0_u8; bytes]).unwrap();
        path
    }

    #[test]
    fn purging_removes_every_prompt_cache_and_reports_the_bytes_freed() {
        let temp = tempfile::tempdir().unwrap();
        let first = seed_cache(temp.path(), &CATALOG[0], 512);
        let second = seed_cache(temp.path(), &CATALOG[1], 1_024);

        let (removed, bytes) = purge_runtime_caches(temp.path()).unwrap();

        assert_eq!(removed, 2);
        assert_eq!(bytes, 1_536);
        assert!(!first.exists());
        assert!(!second.exists());
    }

    /// The cache directory only exists once `keep_ready` has run at least once.
    #[test]
    fn purging_a_cold_runtime_is_a_no_op_rather_than_an_error() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(purge_runtime_caches(temp.path()).unwrap(), (0, 0));
    }

    #[test]
    fn purging_leaves_the_model_weights_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let model = &CATALOG[0];
        let (final_path, _) = model_paths(temp.path(), model);
        std::fs::write(&final_path, b"GGUF").unwrap();
        seed_cache(temp.path(), model, 8);

        assert_eq!(purge_runtime_caches(temp.path()).unwrap(), (1, 8));
        assert!(
            final_path.is_file(),
            "unload frees runtime state, it never deletes a downloaded model"
        );
    }

    #[test]
    fn list_reports_the_warm_cache_size_per_model() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(runtime_cache_bytes(temp.path(), &CATALOG[0]), 0);
        seed_cache(temp.path(), &CATALOG[0], 64);
        assert_eq!(runtime_cache_bytes(temp.path(), &CATALOG[0]), 64);
        assert_eq!(runtime_cache_bytes(temp.path(), &CATALOG[1]), 0);
    }

    /// A `llama-cli` the user started on their own models must survive; only a
    /// process pointed at the managed directory is ours to kill.
    #[test]
    fn only_processes_referencing_the_managed_models_directory_are_selected() {
        let table = concat!(
            "  101 /usr/local/bin/llama-cli --model /home/me/my-own/model.gguf\n",
            "  202 /opt/autostand/llama-completion --model /state/autostand/models/local/x.gguf\n",
            "  303 /Applications/Autostand.app/Contents/MacOS/autostand-app\n",
        );

        assert_eq!(
            model_holding_pids(table, "/state/autostand/models/local", 1),
            vec![202]
        );
    }

    #[test]
    fn pid_selection_skips_the_calling_process_and_unparsable_rows() {
        let table = concat!(
            "not-a-pid /state/models/local/x.gguf\n",
            "\n",
            "  777 llama-completion --model /state/models/local/x.gguf\n",
            "  888 llama-completion --model /state/models/local/y.gguf\n",
        );

        assert_eq!(
            model_holding_pids(table, "/state/models/local", 777),
            vec![888]
        );
    }

    /// An empty needle would match every command line on the machine.
    #[test]
    fn an_unresolvable_models_directory_selects_nothing() {
        assert!(model_holding_pids("  1 anything\n", "", 0).is_empty());
    }

    #[tokio::test]
    async fn unload_reports_only_what_it_actually_released() {
        let temp = tempfile::tempdir().unwrap();
        seed_cache(temp.path(), &CATALOG[0], 256);

        let summary = unload_runtime(temp.path()).await.unwrap();

        assert_eq!(summary.caches_removed, 1);
        assert_eq!(summary.bytes_freed, 256);
        // Nothing on the machine can reference this fresh temp directory.
        assert_eq!(summary.processes_terminated, 0);
        assert_eq!(
            unload_runtime(temp.path()).await.unwrap(),
            LocalRuntimeUnload::default()
        );
    }

    /// The unload reads every command line on the machine; not one byte of that
    /// may reach the panel.
    #[tokio::test]
    async fn unload_never_prints_a_process_table_into_the_terminal() {
        let temp = tempfile::tempdir().unwrap();
        let sink = std::sync::Arc::new(autostand_runlog::RecordingSink::new());
        let as_ref: autostand_runlog::SinkRef = sink.clone();

        autostand_runlog::scoped(as_ref, async {
            unload_runtime(temp.path()).await.unwrap();
        })
        .await;

        assert!(
            sink.lines().is_empty(),
            "the process probes must stay silent: {:?}",
            sink.messages()
        );
    }

    #[test]
    fn unload_summary_serializes_with_the_contract_field_names() {
        let value = serde_json::to_value(LocalRuntimeUnload::default()).unwrap();
        let object = value.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["bytes_freed", "caches_removed", "processes_terminated"]
        );
    }

    #[test]
    fn deleting_the_selected_model_disables_and_unprioritizes_local_ai() {
        let mut config = crate::commands::types::AppConfig::default();
        activate_local_provider(&mut config, "qwen3.5:2b");
        config.llm.provider_order.push("claude".to_string());

        deactivate_local_provider(&mut config, "qwen3.5:2b");

        assert_eq!(config.llm.preferred_provider, "claude");
        assert!(!config
            .llm
            .provider_order
            .iter()
            .any(|provider| provider == LOCAL_PROVIDER_ID));
        let provider = config
            .llm
            .providers
            .iter()
            .find(|provider| provider.id == LOCAL_PROVIDER_ID)
            .unwrap();
        assert!(!provider.enabled);
        assert!(provider.model.is_empty());
    }
}
