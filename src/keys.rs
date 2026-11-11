// Shared with build.rs (`include!`), so plain comments only.

/// A secret key (`sk_live_…`, `sk_test_…`) can do anything on your account and
/// must never ship inside an app; publishable keys (`pk_live_…`, `pk_test_…`,
/// and older `ak_…` ones) are made for it. Pure.
pub fn is_secret_key(key: &str) -> bool {
    key.trim_start().starts_with("sk_")
}

/// Why a secret key is refused.
pub const SECRET_KEY_IN_APP: &str = "a secret key (sk_…) must never ship in an app: use a publishable key (pk_live_… or pk_test_…), or create sessions from your backend and pass embedToken";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_secret_keys_are_refused() {
        for k in ["sk_live_abc", "sk_test_abc", " sk_live_x"] {
            assert!(is_secret_key(k), "{k}");
        }
        for k in ["pk_live_abc", "pk_test_abc", "ak_legacy", "", "gdemb_x"] {
            assert!(!is_secret_key(k), "{k}");
        }
    }
}
