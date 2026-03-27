# Facebook E2E Smoke Report

Run ID: 20260327-143126

Target: www.facebook.com
Proxy: http://localhost:3128
Artifacts: tests/artifacts/facebook-e2e/20260327-143126

| Stage | Label | Status | Description |
| --- | --- | --- | --- |
| S00 | Preflight | PASS | Services healthy; category disabled; LLM ready via local |
| S01 | Client Initial Request | FAIL | Initial request not blocked (HTTP 200) |
| S02 | Squid Logs | FAIL | CONNECT www.facebook.com missing in squid logs |
| S03 | ICAP Logs | PASS | ICAP handled subdomain:www.facebook.com |
| S04 | Policy Engine Decision | PASS | Policy decision endpoint returned action |
| S05 | Redis Streams | PASS | Redis streams contain subdomain:www.facebook.com |
| S06 | Page Fetcher Logs | FAIL | Page fetch logs missing subdomain:www.facebook.com |
| S07 | Crawl4AI Logs | WARN | Crawl4AI logs missing facebook.com |
| S08 | LLM Worker Logs | FAIL | LLM logs missing subdomain:www.facebook.com |
| S09 | Database State | PASS | DB rows exist for facebook.com |
| S10 | Client Follow-up Request | FAIL | Follow-up request not blocked (HTTP 200) |
| S11 | ICAP Final Decision | PASS | ICAP cache decision emitted |

## Checklist
- [x] S00 Preflight — PASS
- [ ] S01 Client Initial Request — FAIL
- [ ] S02 Squid Logs — FAIL
- [x] S03 ICAP Logs — PASS
- [x] S04 Policy Engine Decision — PASS
- [x] S05 Redis Streams — PASS
- [ ] S06 Page Fetcher Logs — FAIL
- [ ] S07 Crawl4AI Logs — WARN
- [ ] S08 LLM Worker Logs — FAIL
- [x] S09 Database State — PASS
- [ ] S10 Client Follow-up Request — FAIL
- [x] S11 ICAP Final Decision — PASS
