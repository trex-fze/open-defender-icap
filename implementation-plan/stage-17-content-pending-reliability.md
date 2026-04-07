# Stage 17 Implementation Plan - ContentPending Reliability and Terminalization

**Status**: Complete  
**Primary Owners**: Classification + SWG + SRE + QA  
**Created**: 2026-04-07

## Objectives
- Stabilize `ContentPending` end-to-end flow so pending requests reliably transition to terminal classification.
- Make timeout/failure diagnostics actionable in one run.
- Define measurable reliability targets for smoke and production-like environments.

## Scope
1. Reproduce intermittent `tests/content-pending-smoke.sh` timeout after pending enqueue.
2. Add diagnostics and correlation coverage across ICAP -> page-fetcher -> llm-worker -> persistence.
3. Harden worker/queue retry and timeout behavior where needed.
4. Make smoke evidence deterministic and triage-friendly.

## Non-Goals
- No redesign of policy action semantics (completed in Stage 16).
- No remote push/PR workflow changes.

## Work Breakdown
| Task ID | Description | Owner | Status | Notes |
| --- | --- | --- | --- | --- |
| S17-T1 | Establish baseline reliability matrix (10-20 smoke runs) with per-stage timings | QA + SRE | [x] | Initial 10-run baseline captured (40% pass) and analyzed for dominant failure modes. |
| S17-T2 | Add smoke-timeout diagnostics bundle (DB snapshots + service logs) | QA + SWG | [x] | Implemented in `tests/content-pending-smoke.sh` via `capture_timeout_diagnostics`. |
| S17-T3 | Add correlation and latency metrics for pending-to-terminal transitions | Classification + SRE | [x] | Added `llm_pending_age_seconds`, `llm_terminalization_latency_seconds`, and status/terminal counters in worker metrics. |
| S17-T4 | Harden worker retry/timeout semantics to avoid non-terminal stalls | Classification | [x] | Added bounded queue requeue attempts (`OD_LLM_JOB_REQUEUE_MAX`, default 6) with terminal persistence on exhaustion/publish failure. |
| S17-T5 | Tune smoke script wait budgets and auto-dump diagnostics for triage | QA | [x] | Added configurable waits and automatic diagnostics on timeout. |
| S17-T6 | Document runbook for delayed terminalization triage | SRE + Docs | [x] | Added troubleshooting procedure and evidence paths in Stage 10 runbook. |
| S17-T7 | Final reliability gate and verification log | QA + SRE | [x] | Post-hardening 10-run gate reached 90% pass (9/10), meeting target threshold. |

## Reliability Targets
- Smoke reliability target: >= 90% pass over 10 consecutive `content-pending` runs.
- No untriaged timeout: every timeout must produce diagnostics bundle.
- Pending-age target in compose smoke: no `waiting_content` entry older than 5 minutes for target domain.

## Evidence
- Smoke artifacts: `tests/artifacts/content-pending/*`
- Verification log: `implementation-plan/stage-17-verification.md`
