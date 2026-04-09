# Proxy 403 RCA - Docker Desktop (2026-04-09)

## Scope

- Proxy host: `192.168.1.103` (macOS + Docker Desktop)
- Client host: `192.168.1.253` (Linux)
- Proxy endpoint: `http://192.168.1.103:3128`

## Incident summary

Client could reach TCP port `3128`, but proxy requests returned `403 Forbidden`.

This was not a network reachability outage. It was an ACL authorization mismatch at the HAProxy edge before backend forwarding to Squid.

## Architecture path (effective runtime)

`User/Client -> HAProxy (:3128) -> Squid (:3128) -> ICAP adaptor (:1344) -> Policy Engine`

## Symptoms observed

- Client/browser and curl requests returned `403`.
- HAProxy logs showed frontend deny signature with no backend server selected:
  - `forward_proxy/<NOSRV> ... 403`
- Squid accepted no corresponding allow path for these denied requests.

## Evidence snapshot

1. **HAProxy ACL gate exists**
   - Generated config contained:
     - `acl allowed_client src ...`
     - `http-request deny unless allowed_client`

2. **Container-visible source mismatch**
   - Host socket view showed client peer as `192.168.1.253`.
   - Inside `docker-haproxy-1`, active peers/logs showed rewritten source in `172.x` space.

3. **Deny location confirmed**
   - HAProxy logs repeatedly showed `<NOSRV> ... 403`, which confirms deny at frontend ACL stage (before Squid backend).

## Root cause

Docker Desktop on macOS rewrote source IP as seen inside the HAProxy/Squid containers. ACLs were configured for LAN CIDR (`192.168.1.0/24`) using `src` matching, so requests were denied because the container-visible source did not match that CIDR.

## Corrective action implemented

Development profile applied:

- `OD_SQUID_ALLOWED_CLIENT_CIDRS=0.0.0.0/0`
- Recreated `haproxy` and `squid` containers to force env/config regeneration.

Both generated configs reflected the new ACL profile:

- HAProxy: `acl allowed_client src 0.0.0.0/0`
- Squid: `acl localnet src 0.0.0.0/0`

## Post-fix validation

1. HTTP proxy test passed:
   - `curl -x http://192.168.1.103:3128 http://detectportal.firefox.com/success.txt` -> `200`

2. HTTPS CONNECT proxy test passed:
   - `curl -x http://192.168.1.103:3128 https://www.google.com` -> `200`

3. HAProxy logs showed backend routing (healthy path), not frontend deny:
   - `forward_proxy squid_backend/squid ... 200 ...`

4. Squid logs showed successful request classes:
   - `TCP_MISS/200`
   - `TCP_TUNNEL/200`

## Risk and compensating control (dev only)

`0.0.0.0/0` is intentionally broad and should be used only for Docker Desktop development where source-IP ACL fidelity is not reliable.

Compensating control required:

- Restrict inbound `3128/tcp` to trusted LAN ranges at host/router firewall.

## Production recommendation

- Run proxy edge on Linux hosts where client source IP is preserved.
- Revert to strict CIDRs (`192.168.1.0/24` or tighter `/32`) for `OD_SQUID_ALLOWED_CLIENT_CIDRS`.
- Validate identity behavior using:
  - `EXPECTED_CLIENT_IP=<client-ip> tests/proxy-production-linux-e2e.sh`

## Related documentation updates

- `README.md` (architecture + ACL profile notes)
- `docs/architecture.md` (end-to-end flow + Docker Desktop caveat)
- `docs/fast-testing-deployment.md` (profiles, RCA FAQ, validation)
- `docs/runbooks/stage10-web-admin-operator-runbook.md` (troubleshooting guidance)
- `deploy/docker/README.md` (compose troubleshooting note)
