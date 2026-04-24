# FAQ

Operational Q&A for setup, classification behavior, and troubleshooting.

**Q: How do I log in to the Admin UI?**  
Open `/login` and use local credentials (`admin` + `OD_DEFAULT_ADMIN_PASSWORD` from `.env`). In compose HTTPS mode (`https://localhost:19001`), keep `VITE_ADMIN_API_URL` empty so requests stay same-origin.

**Q: Why does `odctl` say "No stored session"?**  
Run `odctl auth login --client-id ...` to create a session, or pass `--token $OD_ADMIN_TOKEN` per command.

**Q: Where do dashboards live?**  
Import `deploy/kibana/dashboards/ip-analytics.ndjson` into Kibana. Prometheus is available at `http://localhost:9090`.

**Q: What changed in Stage 25?**  
Classification uses visible-only extracted text, hidden prompt-injection content is stripped, and suspicious payloads are forced to `Review` with confidence capping. Policy-engine remains enforcement authority.

**Q: How do I validate Stage 25 in a live stack?**  
Run `make start` and then `tests/security/llm-prompt-smoke.sh`. A pass means the injected sample is persisted with action `Review`; store artifacts under `tests/artifacts/security/`.

**Q: Which metrics confirm Stage 25 guardrails?**  
Watch `llm_prompt_injection_marker_total` and `llm_prompt_injection_guardrail_total`. Use `llm_provider_invocations_total` for provider context.

**Q: Can I prevent excerpt text from being sent to online providers?**  
Yes. Set `OD_LLM_ONLINE_CONTEXT_MODE=metadata_only`. That mode keeps online requests metadata-only and applies conservative action/confidence limits.

**Q: What if a site has no renderable page content?**  
Use `OD_LLM_CONTENT_REQUIRED_MODE=auto` with `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD=2` to permit metadata-only fallback after repeated fetch failures.

**Q: Why does a domain stay in Pending Sites?**  
Usually because content is missing or queue replay was interrupted. Keep `OD_PENDING_RECONCILE_ENABLED=true` so stale `waiting_content` keys are re-enqueued.

**Q: Why did classified sites return to pending after restart/day rollover?**  
This is often refresh behavior (TTL + reclass), not data loss. See tuning guidance in `README.md` and variable details in `docs/env-vars-reference.md`.

**Q: Local LLM is up, but it gets no requests. Why?**  
Jobs may still be waiting for content. Use `OD_LLM_CONTENT_REQUIRED_MODE=auto` and `OD_LLM_METADATA_ONLY_ALLOWED_FOR=all` so processing can continue when fetch fails repeatedly.

**Q: What if local LLM returns invalid JSON?**  
The worker attempts metadata-only online verification. If verification fails, the key is terminalized as `unknown-unclassified / insufficient-evidence` to avoid infinite loops.

**Q: Why did a key move to `unknown-unclassified / insufficient-evidence`?**  
That is the safety terminal state after repeated no-content/fetch failures or output-invalid verification failures.

**Q: Recommended local-first profile?**  
Set `OD_LLM_FAILOVER_POLICY=safe`, `OD_LLM_STALE_PENDING_MINUTES=0`, `OD_LLM_CONTENT_REQUIRED_MODE=auto`, and `OD_LLM_METADATA_ONLY_ALLOWED_FOR=all`.

**Q: Where do I see crawl outcomes for a URL?**  
Check `logs/crawl4ai/crawl-audit.jsonl` for per-request `success|failed|blocked`, reason, status, and duration.

**Q: How do I block a full domain including subdomains?**  
Create one `block` override for the apex domain (for example `domain:example.com`). It applies to subdomains unless a more-specific override exists.

**Q: Can I override one subdomain under a blocked domain?**  
Yes. Add a more-specific override (for example `domain:safe.example.com`); most-specific scope wins.

**Q: Why do I see `domain:example.com` when traffic was `www.example.com`?**  
Classification is domain-first. Subdomains are normalized to `domain:<registered_domain>` keys for deduplication.

**Q: Are subdomain controls still possible?**  
Yes. Overrides still support subdomain-specific rules, and those rules outrank parent-domain overrides.

**Q: How do I discover override CLI commands?**  
Run `odctl --help`, `odctl override --help`, and `odctl override create --help`.

**Q: Where is evidence tracked?**  
Stage 7 evidence: `docs/evidence/stage07-checklist.md`. Security smoke workflow: `docs/testing/security-plan.md`.
