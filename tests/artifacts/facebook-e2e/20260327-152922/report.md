# Facebook E2E Smoke Report

Run ID: 20260327-152922

Target: www.facebook.com
Proxy: http://localhost:3128
Artifacts: tests/artifacts/facebook-e2e/20260327-152922

| Stage | Label | Status | Description |
| --- | --- | --- | --- |
| S00 | Preflight | PASS | Services healthy; category disabled; LLM ready via local |
| S01 | Client Initial Request | PASS | Initial request blocked with HTTP 000 (curl exit 56) |
| S02 | Squid Logs | FAIL | CONNECT www.facebook.com missing in squid logs |
| S03 | ICAP Logs | PASS | ICAP handled subdomain:www.facebook.com |
| S04 | Policy Engine Decision | PASS | Policy decision endpoint returned action |
| S05 | Redis Streams | PASS | Redis streams contain subdomain:www.facebook.com |
| S06 | Page Fetcher Logs | FAIL | Page fetch logs missing subdomain:www.facebook.com |
| S07 | Crawl4AI Logs | WARN | Crawl4AI logs missing facebook.com |
| S08 | LLM Worker Logs | FAIL | LLM logs missing subdomain:www.facebook.com |
| S09 | Database State | FAIL | No DB rows for facebook.com |
| S10 | Client Follow-up Request | PASS | Follow-up request blocked with HTTP 000 (curl exit 56) |
| S11 | ICAP Final Decision | PASS | ICAP cache decision emitted |

## Checklist
- [x] S00 Preflight — PASS
- [x] S01 Client Initial Request — PASS
- [ ] S02 Squid Logs — FAIL
- [x] S03 ICAP Logs — PASS
- [x] S04 Policy Engine Decision — PASS
- [x] S05 Redis Streams — PASS
- [ ] S06 Page Fetcher Logs — FAIL
- [ ] S07 Crawl4AI Logs — WARN
- [ ] S08 LLM Worker Logs — FAIL
- [ ] S09 Database State — FAIL
- [x] S10 Client Follow-up Request — PASS
- [x] S11 ICAP Final Decision — PASS
