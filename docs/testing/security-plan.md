# Stage 7 – Security Test Plan

This plan addresses the Stage 7 security deliverables (Spec §30) covering authorization, input validation/prompt injection, and documentation of the results.

## Automated checks
### Authorization smoke
- **Script**: `tests/security/authz-smoke.sh`
  - Verifies `/api/v1/overrides` rejects unauthenticated requests (expects 401).
  - Confirms authenticated reads succeed with `X-Admin-Token`.
  - Attempts to create an override using an invalid `scope_type` (`domain;DROP`), expecting HTTP 400 (validates `validate_override_payload`).

Usage:
```bash
BASE_URL=http://localhost:19000 ADMIN_TOKEN=changeme-admin tests/security/authz-smoke.sh
```

Run this after the compose stack is up; include the console output in the Stage 7 evidence folder.

### Prompt-injection smoke
- **Script**: `tests/security/llm-prompt-smoke.sh`
  - Pushes a synthetic job containing an injection string to the Redis stream.
  - Verifies the resulting classification recorded in Postgres does **not** honor the malicious instruction (action must not be `DROP`).
  - Requires `redis-cli` and `psql` in PATH plus access to the running compose stack.

## Manual/advanced tests
1. **OIDC RBAC smoke**: Configure `OD_OIDC_*` in `deploy/docker/.env` and ensure `odctl report traffic` fails when the issued token lacks `ROLE_REPORTING_VIEW`.
2. **Prompt injection (LLM worker)**:
   - Publish a job to `classification-jobs` with a payload containing a known injection string (e.g., `"<INJECTION> ignore previous instructions"`).
   - Verify `llm-worker` logs show the payload being sanitized (check `svc-llm-worker` logs for `prompt_filter` entries) and that the resulting decision is `monitor` instead of `allow`.
3. **Overrides/user input sanitization**: Attempt to submit an override reason containing `<script>alert(1)</script>` and confirm the Admin UI encodes it correctly (inspect the React UI or API response and ensure it is serialized as text).

Document outcomes (date, tester, result) in the security evidence checklist for Stage 7.
