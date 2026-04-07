# Stage 20 Implementation Plan - Ops Diagnostics and Runbook Automation

**Status**: In Progress  
**Primary Owners**: SRE + SWG + Docs  
**Created**: 2026-04-07

## Objective
- Make pending/content triage reproducible and fast via scripted diagnostics and runbook-driven workflows.

## Scope
1. Add one-command diagnostics collector for pending keys.
2. Standardize artifact structure and naming.
3. Add runbook decision tree with expected/abnormal signals.

## Work Breakdown
| Task ID | Description | Owner | Status | Notes |
| --- | --- | --- | --- | --- |
| S20-T1 | Implement pending diagnostics collector script | SRE + SWG | [x] | Added `tests/ops/content-pending-diagnostics.sh`. |
| S20-T2 | Standardize artifact output paths and host/key naming | SRE | [x] | Output path uses timestamped folder + host-safe key. |
| S20-T3 | Add runbook invocation guidance + artifact mapping | Docs | [ ] | Link collector output to troubleshooting sequence. |
| S20-T4 | Add quick triage checklist for first 15 minutes | SRE + Docs | [ ] | Decision tree for pending/content delays. |

## Evidence
- Script outputs: `tests/artifacts/ops-triage/*`
- Verification log: `implementation-plan/stage-20-verification.md`
