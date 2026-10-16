# Changelog

All notable changes to this project are documented in this file.

## [0.2.0] - 2026-09-07

### Added

- Added OpenCode tool and lifecycle hook support, including allow, deny, rewrite, filter, notification, validation, and plugin integration paths.

### Fixed

- Continued scanning matching rules after a running task overrides an earlier match, preventing later blocking rules from being bypassed.
- Propagated suggestion persistence errors instead of silently reporting successful writes.
- Updated CI to use the current taskit command surface.

### Changed

- Consolidated hook pipeline evaluation and rewrite helpers to reduce duplicated control flow.
- Consolidated enrichment scripts and expanded their fixture coverage.
- Clarified hook short-circuit ordering and refreshed repository guidance.
- Restored isolated release-binary smoke coverage for configured rules, learned failures, and successful post-hooks.

### Non-Conventional Commits

- `c57770e` merge: issue #47 propagate suggestion persistence errors
- `51c5b32` merge: issue #58 clarify hook ordering
- `1b79cfd` merge: issues #66 and #65 consolidate enrichment scripts

## [0.1.0] - 2026-08-26

- Initial release.

[0.2.0]: https://github.com/89jobrien/coursers/compare/v0.1.0...coursers-v0.2.0
[0.1.0]: https://github.com/89jobrien/coursers/releases/tag/v0.1.0
