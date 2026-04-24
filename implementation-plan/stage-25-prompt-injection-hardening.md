# Stage 25 Implementation Plan - Prompt Injection Hardening

**Status**: Planned  
**Depends on**: Stages 14, 16, 17, 21, and 24 hardening foundations  
**Execution checklist**: `implementation-plan/stage-25-checklist.md`  
**Decisions log**: `implementation-plan/stage-25-decisions.md`

## 1) Delivery Strategy

Implement in six phases:

- Phase A: threat model lock and rollout controls
- Phase B: strict visible-only extraction at crawl boundary
- Phase C: LLM prompt-injection detection and forced-review guardrail
- Phase D: action-authority hardening (policy engine as enforcement authority)
- Phase E: security/regression test upgrades
- Phase F: docs, runbooks, verification evidence

## 2) Work Breakdown

| Task ID | Description | Area | Dependencies | Output |
| --- | --- | --- | --- | --- |
| S25-T1 | Publish Stage 25 threat model, accepted attack classes, and rollout controls | Security + Platform | none | approved decisions and rollout defaults |
| S25-T2 | Implement strict visible-only extraction in Crawl4AI service (drop hidden/instructional DOM content) | Crawl + Backend | S25-T1 | sanitized visible text content path |
| S25-T3 | Add Crawl4AI sanitization telemetry fields and logs (`nodes_removed`, marker counts) | Crawl + SRE | S25-T2 | operator-visible sanitization signals |
| S25-T4 | Validate/update page-fetcher excerpt contract with strict crawler output | Classification + Backend | S25-T2 | stable excerpt storage/parsing path |
| S25-T5 | Implement llm-worker injection marker detector and weighted suspicion score | Classification + Security | S25-T1, S25-T4 | deterministic pre-invocation risk scoring |
| S25-T6 | Enforce guardrail policy: high suspicion forces `Review` and confidence cap | Classification + Security | S25-T5 | conservative terminal action for suspicious payloads |
| S25-T7 | Persist new guardrail/detection flags into classification metadata and add metrics | Classification + SRE | S25-T6 | auditable outcomes + dashboards/alerts hooks |
| S25-T8 | Remove direct LLM action enforcement cache writes; publish invalidation-only path | Classification + SWG | S25-T6 | policy engine becomes runtime action authority |
| S25-T9 | Validate ICAP/runtime behavior after authority shift (cache misses, policy re-eval, pending flow) | SWG + Policy + QA | S25-T8 | no enforcement regressions |
| S25-T10 | Replace security smoke vector with real `content_excerpt` injection scenarios | Security + QA | S25-T6 | effective end-to-end attack simulation |
| S25-T11 | Add unit/integration suites for strict extraction + guardrail + authority shift | QA + Backend | S25-T2..S25-T10 | repeatable CI coverage |
| S25-T12 | Update docs/runbooks and publish verification evidence package | Docs + SRE + QA | S25-T11 | operator-ready guidance and evidence trail |

## 3) Phase Plan

### Phase A - Decisions and Controls

- Finalize attack classes for this stage: hidden HTML instruction injection, visible instruction injection, canonical-valid coercion outputs, and obfuscated directive strings.
- Finalize default controls and toggles:
  - `OD_PROMPT_INJECTION_GUARDRAIL_ENABLED=true`
  - `OD_PROMPT_INJECTION_REVIEW_THRESHOLD` (default in decisions file)
  - `OD_PROMPT_INJECTION_CONFIDENCE_CAP` (default in decisions file)
- Exit criteria: decisions file approved and referenced by implementation PRs.

### Phase B - Strict Visible-Only Extraction

- Update crawler extraction so runtime classification text is built from visible DOM text only.
- Remove non-visible and non-content elements before text extraction.
- Keep extraction deterministic and bounded by existing `max_text_chars` controls.
- Exit criteria: hidden payloads (for example, `display:none` injected directives) are not present in crawl output.

### Phase C - Detection and Guardrails

- Add llm-worker marker detection and suspicion scoring on excerpt input.
- Force `Review` when threshold is exceeded; cap confidence and persist reason flags.
- Emit metrics for detection/guardrail outcomes.
- Exit criteria: suspicious excerpt payloads cannot yield permissive final actions.

### Phase D - Action Authority Hardening

- Stop direct enforcement cache writes derived from LLM `recommended_action`.
- Keep classification persistence and cache invalidation signaling; rely on policy-engine runtime evaluation for action.
- Exit criteria: ICAP runtime action is policy-engine authoritative with no direct LLM bypass path.

### Phase E - Security and Regression Validation

- Replace current injection smoke with excerpt-based vectors (including canonical-valid coercion patterns).
- Add targeted integration assertions for guardrail action and persisted flags.
- Exit criteria: smoke and integration tests fail on regression and pass with guardrail behavior.

### Phase F - Documentation and Evidence

- Update security plan docs, runbooks, and FAQ references to reflect implemented behavior.
- Publish stage verification report with commands, outputs, and artifact locations.
- Exit criteria: verification package is complete and reproducible for release checks.

## 4) Implementation Touchpoints

- Crawl extraction and sanitization: `services/crawl4ai-service/app/main.py`, `services/crawl4ai-service/requirements.txt`
- Excerpt persistence contract: `workers/page-fetcher/src/main.rs`
- Guardrails and metrics: `workers/llm-worker/src/main.rs`, `workers/llm-worker/src/metrics.rs`
- Enforcement path validation: `services/icap-adaptor/src/main.rs`, `services/icap-adaptor/src/cache.rs`, `services/policy-engine/src/main.rs`
- Security tests: `tests/security/llm-prompt-smoke.sh`
- Documentation updates: `docs/testing/security-plan.md`, `README.md`, runbook updates as needed

## 5) Validation and Evidence

- Backend/unit: `cargo test --workspace`
- Targeted worker suites:
  - `cargo test -p page-fetcher`
  - `cargo test -p llm-worker`
  - `cargo test -p icap-adaptor`
  - `cargo test -p policy-engine`
- Security smoke:
  - `tests/security/llm-prompt-smoke.sh`
- Optional production-like checks:
  - `tests/content-pending-smoke.sh`
  - `tests/integration.sh`

Evidence targets:

- `implementation-plan/stage-25-verification.md`
- `implementation-plan/stage-25-decisions.md`
- `tests/artifacts/security/*` (new or existing security smoke artifacts)
- Updated docs and runbooks referencing guardrail behavior and limits

## 6) Acceptance Tracking

- [ ] Strict visible-only extraction removes hidden/instructional HTML payloads from crawl text.
- [ ] llm-worker detects prompt-injection markers and records structured suspicion metadata.
- [ ] High-suspicion payloads force `Review` and apply confidence cap.
- [ ] Direct LLM action enforcement cache path is removed; policy engine is runtime action authority.
- [ ] Security smoke uses `content_excerpt` attack vectors and passes reliably.
- [ ] Stage verification document captures reproducible command outputs and artifact links.
