/// Re-export shim — all prefix logic now lives in the `prefixe` crate.
pub use prefixe::*;

/// Backwards-compat alias for `PrefixConfig`.
pub type RxPrefixConfig = prefixe::PrefixConfig;
