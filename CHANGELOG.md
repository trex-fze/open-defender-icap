# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

## [v0.1.0] - 2026-04-18

### Added

- Initial public open-source release baseline for Open Defender ICAP.
- Rust decision plane and worker services for policy evaluation, classification, reclassification, and page-content fetch orchestration.
- Web admin frontend, `odctl` CLI workflows, and API surfaces for operations and governance.
- Docker Compose deployment assets, proxy edge integration, observability stack wiring, and runbook/test documentation.

### Changed

- README quick start and deployment guidance aligned to require LLM provider configuration before first stack startup.
- FAQ coverage expanded for content-first pending/reclassification lifecycle behavior and tuning controls.

### Security

- Public reporting path documented in `SECURITY.md`.
- Community governance and contribution safety baseline added (`CODE_OF_CONDUCT.md`, issue/PR templates, CODEOWNERS).
