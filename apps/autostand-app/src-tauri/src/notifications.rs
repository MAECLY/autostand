//! Cross-platform system notifications.
//!
//! The desktop app uses Tauri's official notification plugin so the operating
//! system owns permission prompts. Scheduled `--compile` runs do not have a
//! Tauri runtime, so they use each platform's native notification command while
//! sharing the exact same preference and deduplication policy.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::commands;
use crate::commands::types::{CompileResult, CompileStatus};
use crate::error::AppError;

const HISTORY_FILE: &str = "notification-history.json";
const DEDUP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const HISTORY_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// User-controlled notification preferences persisted as part of `AppConfig`.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationConfig {
    /// Master opt-in. Permission is requested separately and never implicitly.
    pub enabled: bool,
    /// Alert when a provider reports less usage than `low_usage_threshold_percent`.
    pub low_usage: bool,
    /// Remaining-usage percentage that changes a provider to `low`.
    pub low_usage_threshold_percent: u8,
    /// Alert when a provider has exhausted quota or billing balance.
    pub provider_exhausted: bool,
    /// Alert when a render continues with another provider.
    pub provider_fallback: bool,
    /// Alert when a local-model download completes or fails.
    pub local_model_downloads: bool,
    /// Alert after a successful scheduled/interactive compile.
    pub standup_complete: bool,
    /// Alert when a scheduled/interactive compile fails.
    pub standup_failed: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            low_usage: true,
            low_usage_threshold_percent: 20,
            provider_exhausted: true,
            provider_fallback: true,
            local_model_downloads: true,
            standup_complete: false,
            standup_failed: true,
        }
    }
}

impl NotificationConfig {
    /// Normalize a config loaded from disk or received over IPC.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.low_usage_threshold_percent = self.low_usage_threshold_percent.min(100);
        self
    }

    fn allows(&self, kind: NotificationKind) -> bool {
        self.enabled
            && match kind {
                NotificationKind::LowUsage => self.low_usage,
                NotificationKind::ProviderExhausted => self.provider_exhausted,
                NotificationKind::ProviderFallback => self.provider_fallback,
                NotificationKind::LocalModelDownload => self.local_model_downloads,
                NotificationKind::StandupComplete => self.standup_complete,
                NotificationKind::StandupFailed => self.standup_failed,
                NotificationKind::Test => true,
            }
    }
}

/// Notification classes used for preference filtering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    LowUsage,
    ProviderExhausted,
    ProviderFallback,
    LocalModelDownload,
    StandupComplete,
    StandupFailed,
    Test,
}

/// A safe system notification. Callers must not include standup content,
/// prompts, model responses, API keys, or raw provider errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemNotification {
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    /// Stable transition identifier, such as `grok:exhausted:reset-epoch`.
    pub dedup_key: String,
}

impl SystemNotification {
    #[must_use]
    pub fn new(
        kind: NotificationKind,
        title: impl Into<String>,
        body: impl Into<String>,
        dedup_key: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            title: compact_text(&title.into(), 80),
            body: compact_text(&body.into(), 240),
            dedup_key: dedup_key.into(),
        }
    }

    #[must_use]
    pub fn provider_exhausted(provider_id: &str, reset_key: Option<&str>) -> Self {
        let reset = reset_key.unwrap_or("unknown-reset");
        Self::new(
            NotificationKind::ProviderExhausted,
            "AI usage exhausted",
            format!("{provider_id} has no usage available. Autostand will try the next provider."),
            format!("provider:{provider_id}:exhausted:{reset}"),
        )
    }

    #[must_use]
    pub fn provider_fallback(from: &str, to: &str) -> Self {
        Self::new(
            NotificationKind::ProviderFallback,
            "AI provider changed",
            format!("Autostand continued from {from} with {to}."),
            format!("fallback:{from}:{to}"),
        )
    }

    #[must_use]
    pub fn low_usage(provider_id: &str, remaining_percent: u8, window_key: &str) -> Self {
        Self::new(
            NotificationKind::LowUsage,
            "AI usage is running low",
            format!("{provider_id} has {remaining_percent}% remaining."),
            format!("provider:{provider_id}:low:{window_key}"),
        )
    }

    #[must_use]
    pub fn local_model_download(model: &str, succeeded: bool) -> Self {
        let state = if succeeded { "ready" } else { "failed" };
        let (title, body) = if succeeded {
            ("Local model ready", format!("{model} is ready to use."))
        } else {
            (
                "Local model download failed",
                format!("{model} could not be installed. Retry from Settings."),
            )
        };
        Self::new(
            NotificationKind::LocalModelDownload,
            title,
            body,
            format!("local-model:{model}:{state}"),
        )
    }
}

/// Permission and preference state returned to Settings.
#[derive(Debug, Clone, Serialize)]
pub struct NotificationStatus {
    /// Native notification delivery is implemented on desktop targets.
    pub supported: bool,
    /// `granted`, `denied`, `prompt`, or a future plugin value.
    pub permission: String,
    pub config: NotificationConfig,
}

/// Result of a delivery attempt. Suppression is a successful policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    Disabled,
    Duplicate,
}

trait NotificationSink {
    fn send(&self, title: &str, body: &str) -> Result<(), String>;
}

struct TauriSink<'a>(&'a AppHandle);

impl NotificationSink for TauriSink<'_> {
    fn send(&self, title: &str, body: &str) -> Result<(), String> {
        self.0
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|err| err.to_string())
    }
}

struct HeadlessSink;

impl NotificationSink for HeadlessSink {
    fn send(&self, title: &str, body: &str) -> Result<(), String> {
        send_headless_native(title, body)
    }
}

#[cfg(target_os = "macos")]
fn send_headless_native(title: &str, body: &str) -> Result<(), String> {
    let script =
        "on run argv\n display notification (item 2 of argv) with title (item 1 of argv)\nend run";
    command_succeeded(
        std::process::Command::new("/usr/bin/osascript")
            .args(["-e", script, "--", title, body])
            .status(),
        "osascript",
    )
}

#[cfg(target_os = "linux")]
fn send_headless_native(title: &str, body: &str) -> Result<(), String> {
    command_succeeded(
        std::process::Command::new("notify-send")
            .args(["--app-name=Autostand", title, body])
            .status(),
        "notify-send",
    )
}

#[cfg(target_os = "windows")]
fn send_headless_native(title: &str, body: &str) -> Result<(), String> {
    // Fixed script, dynamic text only through environment variables. Encoding
    // plus SecurityElement::Escape keeps provider/model labels out of code/XML.
    const SCRIPT: &str = r#"$t=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:AUTOSTAND_NOTIFICATION_TITLE));$b=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:AUTOSTAND_NOTIFICATION_BODY));$t=[Security.SecurityElement]::Escape($t);$b=[Security.SecurityElement]::Escape($b);[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]>$null;$x=New-Object Windows.Data.Xml.Dom.XmlDocument;$x.LoadXml(\"<toast><visual><binding template='ToastGeneric'><text>$t</text><text>$b</text></binding></visual></toast>\");$n=[Windows.UI.Notifications.ToastNotification]::new($x);[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('com.miguel50flowers.autostand').Show($n)"#;
    command_succeeded(
        std::process::Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                SCRIPT,
            ])
            .env("AUTOSTAND_NOTIFICATION_TITLE", base64(title.as_bytes()))
            .env("AUTOSTAND_NOTIFICATION_BODY", base64(body.as_bytes()))
            .status(),
        "Windows notification service",
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn send_headless_native(_title: &str, _body: &str) -> Result<(), String> {
    Err("headless notifications are unsupported on this operating system".into())
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn command_succeeded(
    status: Result<std::process::ExitStatus, std::io::Error>,
    service: &str,
) -> Result<(), String> {
    let status = status.map_err(|err| format!("start {service}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{service} exited with {status}"))
    }
}

#[cfg(target_os = "windows")]
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        encoded.push(TABLE[usize::from(a >> 2)] as char);
        encoded.push(TABLE[usize::from(((a & 0x03) << 4) | (b >> 4))] as char);
        encoded.push(if chunk.len() > 1 {
            TABLE[usize::from(((b & 0x0f) << 2) | (c >> 6))] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            TABLE[usize::from(c & 0x3f)] as char
        } else {
            '='
        });
    }
    encoded
}

/// Deliver through the official Tauri plugin when the app runtime is active.
pub fn notify_gui(
    app: &AppHandle,
    config: &NotificationConfig,
    notification: &SystemNotification,
) -> Result<Delivery, AppError> {
    dispatch(
        &TauriSink(app),
        config,
        notification,
        &commands::state_dir().join(HISTORY_FILE),
        now_epoch_secs(),
    )
}

/// Deliver from the scheduler's `--compile` process without a GUI runtime.
pub fn notify_headless(
    config: &NotificationConfig,
    notification: &SystemNotification,
) -> Result<Delivery, AppError> {
    dispatch(
        &HeadlessSink,
        config,
        notification,
        &commands::state_dir().join(HISTORY_FILE),
        now_epoch_secs(),
    )
}

/// Convert a completed pipeline run into one content-free system alert.
/// Delivery failure is intentionally returned to the caller separately: a
/// notification service outage must never change the compile result.
#[must_use]
pub fn compile_notification(
    outcome: Result<&[CompileResult], &AppError>,
) -> Option<SystemNotification> {
    match outcome {
        Ok(results)
            if results
                .iter()
                .any(|result| result.status == CompileStatus::Error) =>
        {
            let dates = result_dates(results);
            Some(SystemNotification::new(
                NotificationKind::StandupFailed,
                "Standup compilation failed",
                "Autostand could not compile every target. Open the app for details.",
                format!("standup:failed:{dates}"),
            ))
        }
        Ok(results) if !results.is_empty() => {
            let dates = result_dates(results);
            Some(SystemNotification::new(
                NotificationKind::StandupComplete,
                "Standup compiled",
                "Autostand finished the scheduled compilation.",
                format!("standup:complete:{dates}"),
            ))
        }
        Ok(_) => None,
        Err(err) => Some(SystemNotification::new(
            NotificationKind::StandupFailed,
            "Standup compilation could not start",
            "Autostand could not run the scheduled compilation. Open the app for details.",
            format!("standup:run-failed:{}", error_class(err)),
        )),
    }
}

/// Derive provider transition alerts from a completed chain. This keeps the
/// notification module decoupled from the renderer while making its structured
/// telemetry immediately useful to GUI and headless callers.
#[must_use]
pub fn provider_notifications(
    attempts: &[crate::render::ProviderAttempt],
    winner: Option<&str>,
) -> Vec<SystemNotification> {
    let mut notifications = Vec::new();
    for attempt in attempts {
        if matches!(
            attempt.reason.as_deref(),
            Some("usage_balance_exhausted" | "payment_required")
        ) {
            notifications.push(SystemNotification::provider_exhausted(
                &attempt.provider,
                None,
            ));
        }
    }
    if let Some(winner) = winner {
        if let Some(first_failed) = attempts.iter().find(|attempt| {
            attempt.provider != winner
                && attempt.status == crate::render::ProviderAttemptStatus::Failed
        }) {
            notifications.push(SystemNotification::provider_fallback(
                &first_failed.provider,
                winner,
            ));
        }
    }
    notifications
}

fn result_dates(results: &[CompileResult]) -> String {
    results
        .iter()
        .map(|result| result.date.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn error_class(err: &AppError) -> &'static str {
    match err {
        AppError::Config(_) => "config",
        AppError::Io(_) => "io",
        AppError::Git(_) => "git",
        AppError::Llm(_) => "llm",
        AppError::Lock(_) => "lock",
        AppError::NotFound(_) => "not-found",
        AppError::Invalid(_) => "invalid",
    }
}

fn dispatch(
    sink: &impl NotificationSink,
    config: &NotificationConfig,
    notification: &SystemNotification,
    history_path: &Path,
    now: u64,
) -> Result<Delivery, AppError> {
    if !config.allows(notification.kind) {
        return Ok(Delivery::Disabled);
    }

    let mut history = read_history(history_path);
    if history
        .sent_at
        .get(&notification.dedup_key)
        .is_some_and(|sent_at| now.saturating_sub(*sent_at) < DEDUP_INTERVAL.as_secs())
    {
        return Ok(Delivery::Duplicate);
    }

    sink.send(&notification.title, &notification.body)
        .map_err(|err| AppError::Config(format!("send system notification: {err}")))?;
    history
        .sent_at
        .retain(|_, sent_at| now.saturating_sub(*sent_at) < HISTORY_RETENTION.as_secs());
    history.sent_at.insert(notification.dedup_key.clone(), now);
    write_history(history_path, &history)?;
    Ok(Delivery::Sent)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NotificationHistory {
    sent_at: BTreeMap<String, u64>,
}

fn read_history(path: &Path) -> NotificationHistory {
    let Ok(bytes) = std::fs::read(path) else {
        return NotificationHistory::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|err| {
        tracing::warn!(path = %path.display(), error = %err, "ignoring corrupt notification history");
        NotificationHistory::default()
    })
}

fn write_history(path: &Path, history: &NotificationHistory) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io("notification history has no parent directory".into()))?;
    std::fs::create_dir_all(parent)?;
    let temp = temp_path(path);
    let bytes = serde_json::to_vec(history)?;
    let mut file = std::fs::File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map_or_else(
        || "notification-history".into(),
        std::ffi::OsStr::to_os_string,
    );
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn compact_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.chars().take(max_chars).collect()
}

fn permission_label(value: impl std::fmt::Debug) -> String {
    let raw = format!("{value:?}");
    raw.chars()
        .enumerate()
        .flat_map(|(index, ch)| {
            if ch.is_ascii_uppercase() && index > 0 {
                vec!['-', ch.to_ascii_lowercase()]
            } else {
                vec![ch.to_ascii_lowercase()]
            }
        })
        .collect()
}

/// Return current OS permission and saved preferences without prompting.
#[tauri::command]
pub async fn get_notification_status(
    app_handle: AppHandle,
) -> Result<NotificationStatus, AppError> {
    let permission = app_handle
        .notification()
        .permission_state()
        .map_err(|err| AppError::Config(format!("read notification permission: {err}")))?;
    let config = commands::load_config(&app_handle)?
        .notifications
        .normalized();
    Ok(NotificationStatus {
        supported: true,
        permission: permission_label(permission),
        config,
    })
}

/// Ask the OS for notification permission after an explicit Settings action.
#[tauri::command]
pub async fn request_notification_permission(app_handle: AppHandle) -> Result<String, AppError> {
    let permission = app_handle
        .notification()
        .request_permission()
        .map_err(|err| AppError::Config(format!("request notification permission: {err}")))?;
    Ok(permission_label(permission))
}

/// Send a harmless test alert. It bypasses category switches but still requires
/// the master opt-in and OS permission.
#[tauri::command]
pub async fn send_test_notification(app_handle: AppHandle) -> Result<bool, AppError> {
    let config = commands::load_config(&app_handle)?
        .notifications
        .normalized();
    let notification = SystemNotification::new(
        NotificationKind::Test,
        "Autostand notifications are ready",
        "Usage and provider alerts will appear here.",
        format!("test:{}", now_epoch_secs()),
    );
    Ok(notify_gui(&app_handle, &config, &notification)? == Delivery::Sent)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::{
        compact_text, compile_notification, dispatch, Delivery, NotificationConfig,
        NotificationKind, NotificationSink, SystemNotification, DEDUP_INTERVAL,
    };
    use crate::pipeline_runner::{base_result, error_result};

    #[derive(Default)]
    struct FakeSink(RefCell<Vec<(String, String)>>);

    impl NotificationSink for FakeSink {
        fn send(&self, title: &str, body: &str) -> Result<(), String> {
            self.0
                .borrow_mut()
                .push((title.to_string(), body.to_string()));
            Ok(())
        }
    }

    fn enabled() -> NotificationConfig {
        NotificationConfig {
            enabled: true,
            ..NotificationConfig::default()
        }
    }

    #[test]
    fn defaults_are_opt_in_and_use_the_twenty_percent_threshold() {
        let config = NotificationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.low_usage_threshold_percent, 20);
        assert!(!config.standup_complete);
        assert!(config.provider_exhausted);
    }

    #[test]
    fn normalizes_an_out_of_range_threshold() {
        let config = NotificationConfig {
            low_usage_threshold_percent: u8::MAX,
            ..NotificationConfig::default()
        };
        assert_eq!(config.normalized().low_usage_threshold_percent, 100);
    }

    #[test]
    fn disabled_notifications_never_reach_the_transport() {
        let dir = tempfile::tempdir().unwrap();
        let sink = FakeSink::default();
        let result = dispatch(
            &sink,
            &NotificationConfig::default(),
            &SystemNotification::provider_exhausted("grok", None),
            &dir.path().join("history.json"),
            1_000,
        )
        .unwrap();
        assert_eq!(result, Delivery::Disabled);
        assert!(sink.0.borrow().is_empty());
    }

    #[test]
    fn suppresses_the_same_transition_for_six_hours() {
        let dir = tempfile::tempdir().unwrap();
        let history = dir.path().join("history.json");
        let sink = FakeSink::default();
        let note = SystemNotification::provider_exhausted("grok", Some("window-1"));
        assert_eq!(
            dispatch(&sink, &enabled(), &note, &history, 10_000).unwrap(),
            Delivery::Sent
        );
        assert_eq!(
            dispatch(&sink, &enabled(), &note, &history, 10_001).unwrap(),
            Delivery::Duplicate
        );
        assert_eq!(sink.0.borrow().len(), 1);

        assert_eq!(
            dispatch(
                &sink,
                &enabled(),
                &note,
                &history,
                10_000 + DEDUP_INTERVAL.as_secs()
            )
            .unwrap(),
            Delivery::Sent
        );
        assert_eq!(sink.0.borrow().len(), 2);
    }

    #[test]
    fn a_new_reset_window_is_a_new_transition() {
        let dir = tempfile::tempdir().unwrap();
        let history = dir.path().join("history.json");
        let sink = FakeSink::default();
        let first = SystemNotification::provider_exhausted("grok", Some("window-1"));
        let second = SystemNotification::provider_exhausted("grok", Some("window-2"));
        assert_eq!(
            dispatch(&sink, &enabled(), &first, &history, 50).unwrap(),
            Delivery::Sent
        );
        assert_eq!(
            dispatch(&sink, &enabled(), &second, &history, 51).unwrap(),
            Delivery::Sent
        );
    }

    #[test]
    fn category_switches_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        let sink = FakeSink::default();
        let mut config = enabled();
        config.provider_fallback = false;
        let note = SystemNotification::provider_fallback("grok", "openai");
        assert_eq!(
            dispatch(&sink, &config, &note, &dir.path().join("h"), 1).unwrap(),
            Delivery::Disabled
        );
    }

    #[test]
    fn notification_text_is_single_line_and_bounded() {
        let notification = SystemNotification::new(
            NotificationKind::Test,
            "  title\n with   space ",
            "x".repeat(300),
            "test",
        );
        assert_eq!(notification.title, "title with space");
        assert_eq!(notification.body.chars().count(), 240);
        assert_eq!(compact_text(" a\n b ", 10), "a b");
    }

    #[test]
    fn compile_alerts_never_include_file_or_error_details() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 8, 13).unwrap();
        let failed = error_result(date, "host", "secret provider response");
        let notification = compile_notification(Ok(&[failed])).unwrap();
        assert_eq!(notification.kind, NotificationKind::StandupFailed);
        assert!(!notification.body.contains("secret"));

        let succeeded = base_result(date, "host", std::path::Path::new("/private/daily.md"));
        let notification = compile_notification(Ok(&[succeeded])).unwrap();
        assert_eq!(notification.kind, NotificationKind::StandupComplete);
        assert!(!notification.body.contains("daily.md"));
    }

    #[test]
    fn provider_attempts_produce_exhausted_and_fallback_alerts() {
        use crate::render::{ProviderAttempt, ProviderAttemptStatus};

        let attempts = vec![
            ProviderAttempt {
                provider: "grok".into(),
                channel: None,
                model: "grok-code-fast-1".into(),
                status: ProviderAttemptStatus::Failed,
                reason: Some("usage_balance_exhausted".into()),
                latency_ms: Some(1),
            },
            ProviderAttempt {
                provider: "openai".into(),
                channel: None,
                model: "gpt".into(),
                status: ProviderAttemptStatus::Succeeded,
                reason: None,
                latency_ms: Some(2),
            },
        ];
        let notifications = super::provider_notifications(&attempts, Some("openai"));
        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].kind, NotificationKind::ProviderExhausted);
        assert_eq!(notifications[1].kind, NotificationKind::ProviderFallback);
    }
}
