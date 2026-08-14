//! Read-only discovery of the `OpenRouter` API key.
//!
//! `OpenRouter` has no companion CLI that leaves a credential in a known place,
//! so there is nothing local to reuse. The key is the user's own, read from the
//! one credential path this repository already sanctions — autostand's keychain
//! item, service `autostand`, account `openrouter` — with the vendor's standard
//! environment variables as the fallback.
//!
//! Nothing here writes: saving a key stays with the Settings command.

use crate::usage::creds::{user_key, Secret};
use crate::usage::model::UsageError;

use super::PROVIDER_ID;

/// Keychain account under the `autostand` service.
pub const KEYCHAIN_ACCOUNT: &str = PROVIDER_ID;

/// Environment variables checked in order. `OPENROUTER_API_KEY` is the de-facto
/// standard; `OPENROUTER_KEY` is the shorter form some setups export.
pub const ENV_NAMES: &[&str] = &["OPENROUTER_API_KEY", "OPENROUTER_KEY"];

/// The user's key, if one is configured.
pub async fn api_key() -> Result<Option<Secret>, UsageError> {
    user_key::load(KEYCHAIN_ACCOUNT, ENV_NAMES).await
}

/// Whether a key exists, without reading one.
pub async fn has_credentials() -> bool {
    user_key::exists(KEYCHAIN_ACCOUNT, ENV_NAMES).await
}

#[cfg(test)]
mod tests {
    use super::{ENV_NAMES, KEYCHAIN_ACCOUNT};

    #[test]
    fn the_keychain_account_is_the_provider_id() {
        // Settings writes `("autostand", <provider id>)`; a different account
        // string here would read an item nothing ever writes.
        assert_eq!(KEYCHAIN_ACCOUNT, "openrouter");
    }

    #[test]
    fn the_standard_variable_is_checked_first() {
        assert_eq!(ENV_NAMES, &["OPENROUTER_API_KEY", "OPENROUTER_KEY"]);
    }
}
