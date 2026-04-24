# Stage 25 Decisions - Prompt Injection Hardening

## Decision Log

### D25-1: Extraction Mode
- **Decision**: Enforce strict visible-only extraction at crawl boundary.
- **Status**: Approved.
- **Rationale**: Hidden DOM content is a common remote prompt-injection vector and should not enter classification context.
- **Implementation touchpoint**: `services/crawl4ai-service/app/main.py`.

### D25-2: Guardrail Action
- **Decision**: High prompt-injection suspicion forces `Review`.
- **Status**: Approved.
- **Rationale**: `Review` is safer than permissive outcomes under adversarial uncertainty and preserves analyst visibility.
- **Implementation touchpoint**: `workers/llm-worker/src/main.rs`.

### D25-3: Confidence Governance
- **Decision**: Apply confidence cap when prompt-injection guardrail triggers.
- **Status**: Approved.
- **Rationale**: Reduces overconfident downstream interpretation for suspicious classifications.

### D25-4: Runtime Action Authority
- **Decision**: Remove direct LLM action enforcement cache path; policy engine remains runtime action authority.
- **Status**: Approved.
- **Rationale**: Structural output validity alone is insufficient defense against canonical-valid coercion attacks.
- **Implementation touchpoints**: `workers/llm-worker/src/main.rs`, `services/icap-adaptor/src/main.rs`, `services/policy-engine/src/main.rs`.

### D25-5: Security Smoke Vector
- **Decision**: Replace current smoke payload vector with real `content_excerpt` prompt-injection payloads.
- **Status**: Approved.
- **Rationale**: Unknown JSON fields do not reliably validate real attack paths.
- **Implementation touchpoint**: `tests/security/llm-prompt-smoke.sh`.

## Default Runtime Controls

These values are the Stage 25 defaults unless changed during implementation review:

- `OD_PROMPT_INJECTION_GUARDRAIL_ENABLED=true`
- `OD_PROMPT_INJECTION_REVIEW_THRESHOLD=3`
- `OD_PROMPT_INJECTION_CONFIDENCE_CAP=0.40`

## Deferred Decisions

- Whether to add provider-side secondary adjudication (dual-pass verification) in Stage 25 or subsequent stage.
- Whether to introduce environment-specific threshold overrides in golden profiles by default.

## Non-Goals for Stage 25

- Full semantic truth-verification of page claims.
- Multi-provider consensus architecture for every classification.
- UI-level review workflow redesign.
