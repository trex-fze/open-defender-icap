# Facebook E2E Checklist

- [ ] S00 Preflight: services healthy + social-media disabled + LLM provider reachable
- [ ] S01 Client request sent via Squid proxy to facebook
- [ ] S02 Squid confirms CONNECT request reached proxy
- [ ] S03 ICAP adaptor receives request and publishes jobs
- [ ] S04 Policy Engine direct decision endpoint responds for facebook key
- [ ] S05 Redis streams contain classification + page-fetch jobs
- [ ] S06 Page Fetcher processes facebook job without URL errors
- [ ] S07 Crawl4AI receives crawl request for facebook URL
- [ ] S08 LLM worker processes/classifies facebook key
- [ ] S09 DB shows pending/classification rows for facebook
- [ ] S10 Follow-up client request is blocked/pending (not direct 200 tunnel)
- [ ] S11 ICAP emits cache/final decision for facebook key
- [ ] S12 Consolidated report generated
