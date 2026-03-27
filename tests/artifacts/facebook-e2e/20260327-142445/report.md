# Facebook E2E Smoke Report

Run ID: 20260327-142445

Target: www.facebook.com
Proxy: http://localhost:3128
Artifacts: tests/artifacts/facebook-e2e/20260327-142445

| Stage | Label | Status | Description |
| --- | --- | --- | --- |
| S00 | Preflight | PASS | Services healthy; category disabled; LLM ready via local |
| S01 | Client Initial Request | FAIL | Initial request not blocked (HTTP 200) |
| S02 | Squid Logs | PASS | Squid observed CONNECT www.facebook.com |
| S03 | ICAP Logs | FAIL | ICAP log missing subdomain:www.facebook.com |
| S04 | Policy Engine Decision | PASS | Policy decision endpoint returned action |
| S05 | Redis Streams | PASS | Redis streams contain subdomain:www.facebook.com |
| S06 | Page Fetcher Logs | FAIL | Page fetch logs missing subdomain:www.facebook.com |
| S07 | Crawl4AI Logs | WARN | Crawl4AI logs missing facebook.com |
| S08 | LLM Worker Logs | FAIL | LLM logs missing subdomain:www.facebook.com |
| S09 | Database State | FAIL | No DB rows for facebook.com |
| S10 | Client Follow-up Request | FAIL | Follow-up request not blocked (HTTP 200) |
| S11 | ICAP Final Decision | WARN | No cache decision log found |

## Checklist
- [x] S00 Preflight — PASS
- [ ] S01 Client Initial Request — FAIL
- [x] S02 Squid Logs — PASS
- [ ] S03 ICAP Logs — FAIL
- [x] S04 Policy Engine Decision — PASS
- [x] S05 Redis Streams — PASS
- [ ] S06 Page Fetcher Logs — FAIL
- [ ] S07 Crawl4AI Logs — WARN
- [ ] S08 LLM Worker Logs — FAIL
- [ ] S09 Database State — FAIL
- [ ] S10 Client Follow-up Request — FAIL
- [ ] S11 ICAP Final Decision — WARN
