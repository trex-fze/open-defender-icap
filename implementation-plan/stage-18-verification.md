# Stage 18 Verification Log

Date: 2026-04-07

## Implemented
- Added `tests/security/facebook-e2e-reliability.sh` as a repeat-run harness for facebook e2e smoke.

## Initial run
- Command:
  - `RUNS=1 bash tests/security/facebook-e2e-reliability.sh`
- Result:
  - `pass=0 fail=1 pass_rate=0%`
- Primary failure signatures:
  - `S00` preflight failed because `social-media` category was enabled.
  - `S05`/`S06` stream/fetch stages missed `subdomain:www.facebook.com` in that run window.
  - `S09` DB row check failed for `subdomain:www.facebook.com` while LLM logs still showed classification activity.

## Notes
- This run establishes harness functionality and failure attribution by stage.
- Next step is the formal baseline gate (`RUNS=10`) after preflight normalization (force taxonomy disabled state before each run).

## Pending Verification
- Baseline reliability matrix (`RUNS=10`).
- Post-hardening reliability matrix (`RUNS=10`).
- Failure signature breakdown and triage time validation.
