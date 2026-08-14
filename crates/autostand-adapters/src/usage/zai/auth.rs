//! Read-only discovery of the Z.ai API key.
//!
//! Like `OpenRouter`, Z.ai ships no companion CLI, so the key is the user's own:
//! autostand's keychain item (service `autostand`, account `zai`), with
//! environment variables as the fallback.
//!
//! `ZAI_API_KEY` is the current name; `GLM_API_KEY` is the older Zhipu name some
//! setups still export, so both are accepted in that order.

use crate::usage::creds::{user_key, Secret};
use crate::usage::model::UsageError;

use super::PROVIDER_ID;

/// Keychain account under the `autostand` service.
pub const KEYCHAIN_ACCOUNT: &str = PROVIDER_ID;

/// Environment variables checked in order.
pub const ENV_NAMES: &[&str] = &["ZAI_API_KEY", "GLM_API_KEY"];

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
        assert_eq!(KEYCHAIN_ACCOUNT, "zai");
    }

    #[test]
    fn the_legacy_zhipu_variable_is_a_fallback_not_a_primary() {
        assert_eq!(ENV_NAMES, &["ZAI_API_KEY", "GLM_API_KEY"]);
    }
}
