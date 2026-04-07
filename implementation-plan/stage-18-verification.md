# Stage 18 Verification Log

Date: 2026-04-07

## Implemented
- Added `tests/security/facebook-e2e-reliability.sh` as a repeat-run harness for facebook e2e smoke.
- Added optional failure diagnostics collection hook in reliability harness (`AUTO_COLLECT_DIAGNOSTICS=1`) using `tests/ops/content-pending-diagnostics.sh`.
- Updated facebook e2e smoke checks to enforce canonical-key assertions for stream/page-fetch/db stages while preserving request-path dual-key log matching.

## Initial run
- Command:
  - `RUNS=1 bash tests/security/facebook-e2e-reliability.sh`
- Result:
  - `pass=0 fail=1 pass_rate=0%`
- Primary failure signatures:
  - `S00` preflight failed because `social-media` category was enabled.
  - `S05`/`S06` stream/fetch stages missed `subdomain:www.facebook.com` in that run window.
  - `S09` DB row check failed for `subdomain:www.facebook.com` while LLM logs still showed classification activity.

## Baseline matrix
- Command:
  - `RUNS=10 bash tests/security/facebook-e2e-reliability.sh`
- Result:
  - `pass=0 fail=10 pass_rate=0%`
- Dominant causes:
  - Preflight taxonomy mismatch (`social-media` enabled) and subdomain-key-biased assertions in stream/page-fetch/db stages.

## Post-hardening gate
- Command:
  - `RUNS=10 bash tests/security/facebook-e2e-reliability.sh`
- Result:
  - `pass=10 fail=0 pass_rate=100%`
- Notes:
  - Harness now auto-normalizes preflight taxonomy state (`AUTO_DISABLE_SOCIAL_CATEGORY=1`).
  - Stage summary lines include artifact paths for every run; failed runs trigger optional diagnostics bundles under `tests/artifacts/ops-triage/`.

## Gate status
- Reliability gate met (>=90% pass).
