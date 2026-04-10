# Stage 24 Verification Log

Date: 2026-04-10

## 1) Workspace, Unit, and Frontend Gates

- `cargo test --workspace` -> PASS
- `npm test` (`web-admin`) -> PASS
- `npm run build` (`web-admin`) -> PASS

## 2) Auth Security Smoke Matrix

- `bash tests/security/authz-smoke.sh` -> PASS
  - unauthenticated override access rejected
  - authenticated override access accepted
  - invalid scope payload rejected
  - invalid refresh token rejected
  - password-change route rejected for non-user principal

## 3) Queue Reliability Stress Suite

- Initial run: `RUNS=3 bash tests/security/facebook-e2e-reliability.sh` -> FAIL (pass_rate=33%)
- Hardening applied:
  - added automatic stack bootstrap support to `tests/security/facebook-e2e-reliability.sh`
  - reduced log-only flakiness in `tests/security/facebook-e2e-smoke.sh` by allowing DB/downstream evidence fallback for Crawl4AI and LLM stages
- Final run: `RUNS=3 WAIT_STAGE_SECONDS=45 WAIT_LLM_SECONDS=120 WAIT_DB_SECONDS=180 AUTO_STACK_BOOTSTRAP=0 AUTO_STACK_TEARDOWN=0 bash tests/security/facebook-e2e-reliability.sh` -> PASS (pass_rate=100%)
- Note: generated reliability artifacts are ephemeral and were cleaned after validation.

## 4) Golden Profile Bring-Up/Teardown Drills

- `PROFILE=golden-local bash tests/ops/golden-profile.sh verify` -> PASS
- `PROFILE=golden-prodlike bash tests/ops/golden-profile.sh verify` -> PASS
- `PROFILE=golden-local bash tests/ops/golden-profile.sh down` -> PASS
- `PROFILE=golden-prodlike bash tests/ops/golden-profile.sh down` -> PASS

## 5) Follow-up Actions

1. Keep `RUNS=10` reliability burn-in as a periodic regression gate in CI/nightly.
2. Tune smoke-stage timeout defaults only if future infra profiles materially increase latency.
