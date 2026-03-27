# Facebook E2E Smoke Report

Run ID: 20260327-160516

Target: www.facebook.com
Proxy: http://localhost:3128
Artifacts: tests/artifacts/facebook-e2e/20260327-160516

| Stage | Label | Status | Description |
| --- | --- | --- | --- |
| S00 | Preflight | PASS | Services healthy; category disabled; LLM ready via local |
| S01 | Client Initial Request | PASS | Initial request blocked with HTTP 000 (curl exit 56) |
| S02 | Squid Logs | WARN | Squid stdout lacked CONNECT line; ICAP confirms request path |
| S03 | ICAP Logs | PASS | ICAP handled subdomain:www.facebook.com |
| S04 | Policy Engine Decision | PASS | Policy decision endpoint returned action |
| S05 | Redis Streams | PASS | Redis streams contain subdomain:www.facebook.com |
| S06 | Page Fetcher Logs | PASS | Page fetch processed subdomain:www.facebook.com |
| S07 | Crawl4AI Logs | WARN | Crawl4AI logs missing facebook.com |
| S08 | LLM Worker Logs | PASS | LLM classified subdomain:www.facebook.com |
| S09 | Database State | PASS | DB rows exist for facebook.com |
| S10 | Client Follow-up Request | PASS | Follow-up request blocked with HTTP 000 (curl exit 56) |
| S11 | ICAP Final Decision | WARN | No cache decision log found |

## Checklist
- [x] S00 Preflight — PASS
- [x] S01 Client Initial Request — PASS
- [ ] S02 Squid Logs — WARN
- [x] S03 ICAP Logs — PASS
- [x] S04 Policy Engine Decision — PASS
- [x] S05 Redis Streams — PASS
- [x] S06 Page Fetcher Logs — PASS
- [ ] S07 Crawl4AI Logs — WARN
- [x] S08 LLM Worker Logs — PASS
- [x] S09 Database State — PASS
- [x] S10 Client Follow-up Request — PASS
- [ ] S11 ICAP Final Decision — WARN
