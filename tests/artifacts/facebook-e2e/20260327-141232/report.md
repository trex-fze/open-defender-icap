# Facebook E2E Smoke Report

Run ID: 20260327-141232

Target: www.facebook.com
Proxy: http://localhost:3128
Artifacts: tests/artifacts/facebook-e2e/20260327-141232

| Stage | Label | Status | Description |
| --- | --- | --- | --- |
| S00 | Preflight | PASS | Services healthy; category disabled; LLM ready via local |
| S01 | Client Initial Request | FAIL | Initial request not blocked (HTTP 200) |
| S02 | Squid Logs | PASS | Squid observed CONNECT www.facebook.com |
| S03 | ICAP Logs | PASS | ICAP handled subdomain:www.facebook.com |
| S04 | Policy Engine Logs | FAIL | Policy log missing subdomain:www.facebook.com |
| S05 | Redis Streams | PASS | Redis streams contain subdomain:www.facebook.com |
| S06 | Page Fetcher Logs | WARN | Page fetch attempted but saw url errors |
| S07 | Crawl4AI Logs | WARN | Crawl4AI logs missing facebook.com |
| S08 | LLM Worker Logs | FAIL | LLM logs missing subdomain:www.facebook.com |
| S09 | Database State | FAIL | No DB rows for facebook.com |
| S10 | Client Follow-up Request | FAIL | Follow-up request not blocked (HTTP 200) |
| S11 | ICAP Final Decision | PASS | ICAP cache decision emitted |
