# Stage 7 RFC Addendum – Testing, Deployment & Operations

**Parent Sections**: `docs/engine-adaptor-spec.md` §§8, 24–35.

## Objectives
1. Execute complete test strategy: unit, component, integration, smoke, performance, security, UAT.
2. Finalize Docker-compose/k8s deployment workflows, runbooks, rollback plans.
3. Produce evidence checklist artifacts for signoffs (architecture, test reports, security, QA, ops).

## Checklist
- [x] Unit test coverage per function (Spec §22–§23, §26) – documented via `docs/testing/unit-coverage.md` and enforced with the `tests/unit.sh` runner.
- [x] Smoke & integration suites executed via docker-compose (Spec §27–§28) – `tests/integration.sh` orchestrates `docker-compose up`, runs `odctl smoke` plus the Stage 6 ingest smoke test; `deploy/docker/docker-compose.integration.yml` documents the dedicated services.
- [ ] Performance/load tests hitting KPIs (Spec §29).
- [ ] Security tests (authZ, injection, prompt, fail-open/close) – Spec §30.
- [ ] Deployment/rollback checklists validated (Spec §28 & §35).
- [ ] Evidence artifacts compiled (Spec §29–§31) with signoffs for DoD.

## Traceability Plan
| Requirement | Section | Artifact |
| --- | --- | --- |
| Smoke tests | Spec §27 | `odctl smoke run` logs |
| Performance KPIs | Spec §29 | k6/Gatling reports |
| Security validation | Spec §30 | Pen test findings, remediation evidence |
| DoD per component | Spec §32 | Signoff forms, dashboards |

## Pending Items
- Define CI/CD stages executing each suite.
- Establish artifact storage (S3, SharePoint, etc.) for evidence package.
