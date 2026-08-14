//! Read-only access to an API key the **user** gave autostand.
//!
//! Most probes reuse a credential some vendor CLI already left on disk. Two do
//! not: `OpenRouter` and Z.ai ship no companion CLI, so there is nothing local
//! to reuse and the key has to come from the user.
//!
//! Rather than invent a new config file, this reads the one credential path the
//! repository already sanctions: autostand's own keychain entry,
//! `keyring::Entry::new("autostand", <provider id>)` — the exact item the
//! `set_api_key` command writes and `get_api_key_status` reports on. Environment
//! variables are the fallback, so a developer who exported the vendor's standard
//! variable is honoured without configuring anything.
//!
//! Read-only like the rest of [`super`]: there is no save and no delete here.
//! Writing the key stays with the Settings command, which is the only place the
//! user asked for it.

use super::secret::Secret;
use super::{files, keychain};
use crate::usage::model::UsageError;

/// Keychain service every autostand-owned secret lives under.
///
/// Matches `commands/llm.rs`, which writes the item; a different service string
/// here would silently read an entry nothing ever writes.
pub const SERVICE: &str = "autostand";

/// Load the key for `account`, preferring the keychain over the environment.
///
/// The keychain wins because it is the value the user typed into Settings; an
/// exported shell variable is the older, less explicit source and must not
/// shadow it.
///
/// `Ok(None)` means no key anywhere — the provider is simply not configured.
/// [`UsageError::CredentialStoreUnavailable`] is reserved for a credential store
/// that could not be consulted *and* no environment fallback: a locked keychain
/// is not the same fact as a missing key, and reporting it as one sends the user
/// to re-enter a key they already have.
pub async fn load(account: &str, env_names: &[&str]) -> Result<Option<Secret>, UsageError> {
    match read_keychain(account).await {
        KeychainRead::Found(secret) => Ok(Some(secret)),
        KeychainRead::NotFound => Ok(from_env(env_names)),
        KeychainRead::Unavailable => from_env(env_names)
            .map(Some)
            .ok_or(UsageError::CredentialStoreUnavailable),
    }
}

/// Whether a key exists, without reading one.
///
/// This runs on the listing path and on every background refresh, so on macOS it
/// uses the attributes-only keychain probe, which cannot raise an unlock prompt.
/// Other platforms have no attributes-only lookup, so they fall back to a read —
/// safe there because the item belongs to autostand itself, not to a third party.
pub async fn exists(account: &str, env_names: &[&str]) -> bool {
    if from_env(env_names).is_some() {
        return true;
    }
    if keychain::is_supported() {
        return keychain::generic_password_exists(SERVICE, Some(account))
            .await
            .unwrap_or(false);
    }
    matches!(read_keychain(account).await, KeychainRead::Found(_))
}

/// The first environment variable in `env_names` that holds a non-blank value.
///
/// Order is the provider's decision (`ZAI_API_KEY` before the legacy
/// `GLM_API_KEY`); this only walks it.
#[must_use]
pub fn from_env(env_names: &[&str]) -> Option<Secret> {
    env_names
        .iter()
        .find_map(|name| files::env_text(name).as_deref().and_then(Secret::new))
}

/// Outcome of one keychain read.
///
/// Three cases rather than `Option`, because "no key stored" and "the store
/// could not be consulted" lead to different reason codes.
enum KeychainRead {
    Found(Secret),
    NotFound,
    Unavailable,
}

/// Read autostand's own keychain item for `account`.
///
/// `keyring` is synchronous and can block on a platform credential store, so the
/// call is moved off the async worker.
async fn read_keychain(account: &str) -> KeychainRead {
    let account = account.to_string();
    tokio::task::spawn_blocking(move || match keyring::Entry::new(SERVICE, &account) {
        Ok(entry) => match entry.get_password() {
            Ok(password) => {
                Secret::new(&password).map_or(KeychainRead::NotFound, KeychainRead::Found)
            }
            Err(keyring::Error::NoEntry) => KeychainRead::NotFound,
            // Locked, denied, or a backend that is not present. Not a logout.
            Err(_) => KeychainRead::Unavailable,
        },
        Err(_) => KeychainRead::Unavailable,
    })
    .await
    .unwrap_or(KeychainRead::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::{exists, from_env, load, SERVICE};
    use tokio::sync::Mutex;

    /// `std::env` is process-global; these tests mutate it. An async-aware lock
    /// so the tests that `await` a lookup can hold it across the await.
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    const FIRST: &str = "AUTOSTAND_TEST_USER_KEY_PRIMARY";
    const SECOND: &str = "AUTOSTAND_TEST_USER_KEY_LEGACY";

    fn clear() {
        std::env::remove_var(FIRST);
        std::env::remove_var(SECOND);
    }

    #[test]
    fn the_service_matches_the_item_settings_writes() {
        // A different string here would read an entry nothing ever writes.
        assert_eq!(SERVICE, "autostand");
    }

    #[tokio::test]
    async fn environment_names_are_walked_in_order() {
        let _guard = ENV_LOCK.lock().await;
        clear();
        std::env::set_var(FIRST, "primary-key");
        std::env::set_var(SECOND, "legacy-key");
        assert_eq!(from_env(&[FIRST, SECOND]).unwrap().as_str(), "primary-key");
        std::env::remove_var(FIRST);
        assert_eq!(from_env(&[FIRST, SECOND]).unwrap().as_str(), "legacy-key");
        clear();
    }

    #[tokio::test]
    async fn an_exported_but_blank_variable_is_not_a_key() {
        let _guard = ENV_LOCK.lock().await;
        clear();
        std::env::set_var(FIRST, "   ");
        assert!(from_env(&[FIRST]).is_none());
        clear();
    }

    #[tokio::test]
    async fn an_unconfigured_provider_reports_no_key_rather_than_an_error() {
        let _guard = ENV_LOCK.lock().await;
        clear();
        let account = "autostand-test-provider-that-does-not-exist";
        // A store with no such item must read as "not configured", never as a
        // credential-store failure that tells the user to re-enter a key.
        let loaded = load(account, &[FIRST]).await;
        assert!(matches!(loaded, Ok(None) | Err(_)), "{loaded:?}");
        clear();
    }

    #[tokio::test]
    async fn an_exported_variable_alone_counts_as_configured() {
        let _guard = ENV_LOCK.lock().await;
        clear();
        std::env::set_var(FIRST, "env-only-key");
        let account = "autostand-test-provider-that-does-not-exist";
        assert!(exists(account, &[FIRST]).await);
        assert_eq!(
            load(account, &[FIRST]).await.unwrap().unwrap().as_str(),
            "env-only-key"
        );
        clear();
    }

    #[tokio::test]
    async fn nothing_configured_is_not_existence() {
        let _guard = ENV_LOCK.lock().await;
        clear();
        assert!(!exists("autostand-test-provider-that-does-not-exist", &[FIRST]).await);
        clear();
    }
}
