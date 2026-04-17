# DHCP Auto Proxy Configuration (PAC/WPAD) for Open Defender

This guide explains how to distribute Open Defender proxy settings automatically to client devices using DHCP-delivered PAC/WPAD configuration.

It is written for operators running Open Defender as an enterprise forward-proxy stack (`HAProxy -> Squid -> ICAP adaptor`) and onboarding many clients without per-device manual proxy setup.

Related docs:
- `README.md` (architecture and quick start)
- `docs/infra-config-reference.md` (proxy and deployment knobs)
- `docs/fast-testing-deployment.md` (local smoke and validation flow)

## 1) Architecture and control model

Auto-proxy usually combines these pieces:

1. DHCP announces where clients can find proxy auto-configuration.
2. Clients fetch a PAC file (`application/x-ns-proxy-autoconfig`).
3. PAC logic decides whether to use Open Defender proxy or go `DIRECT`.

For Open Defender deployments, the PAC proxy target should usually be the HAProxy edge listener:

- host: proxy edge FQDN or IP (for example `proxy.corp.example`)
- port: `OD_HAPROXY_BIND_PORT` (default `3128`)

## 2) PAC vs WPAD vs "local-pac-server"

Terminology differs across DHCP vendors and endpoint platforms:

- `PAC URL`: explicit URL to a `.pac` script.
- `WPAD`: discovery model using DHCP and/or DNS (`wpad.<domain>`).
- `local-pac-server`: vendor wording used in some products for "publish PAC URL to clients".

In many environments, DHCP-based WPAD is implemented with option code `252` carrying a PAC URL string.

Important interoperability note:
- Not all OS/browser stacks honor DHCP option 252.
- Some clients prefer DNS WPAD or explicit management policies (GPO/MDM).
- Treat DHCP PAC as one delivery method in a broader endpoint configuration strategy.

## 3) Recommended production pattern

Use a stable internal HTTPS URL for PAC distribution, for example:

- `https://proxy-config.corp.example/proxy.pac`

Then publish that URL via DHCP option 252 (or your platform's named PAC option such as `local-pac-server` if that is how your DHCP product exposes it).

Why this pattern:
- central PAC updates without touching clients
- cleaner certificate and hosting controls
- easier change management and auditability

## 4) PAC file design for Open Defender

## 4.1 Baseline PAC example

```javascript
function FindProxyForURL(url, host) {
  host = host.toLowerCase();

  // Always bypass localhost and local machine names.
  if (isPlainHostName(host) ||
      dnsDomainIs(host, "localhost") ||
      shExpMatch(host, "127.*") ||
      shExpMatch(host, "10.*") ||
      shExpMatch(host, "192.168.*") ||
      shExpMatch(host, "172.16.*") ||
      shExpMatch(host, "172.17.*") ||
      shExpMatch(host, "172.18.*") ||
      shExpMatch(host, "172.19.*") ||
      shExpMatch(host, "172.2?.*") ||
      shExpMatch(host, "172.30.*") ||
      shExpMatch(host, "172.31.*")) {
    return "DIRECT";
  }

  // Optional internal domain bypass examples.
  if (dnsDomainIs(host, ".corp.example") ||
      dnsDomainIs(host, ".internal.example")) {
    return "DIRECT";
  }

  // Primary Open Defender edge, with optional standby.
  return "PROXY proxy.corp.example:3128; PROXY proxy-dr.corp.example:3128; DIRECT";
}
```

Operator notes:
- keep bypass scope minimal; broad `DIRECT` rules weaken policy visibility
- use FQDNs, not ephemeral container IPs
- include a DR proxy only if policy and telemetry requirements are met on that path

## 4.2 Hosting requirements

- Serve with content type `application/x-ns-proxy-autoconfig`.
- Use stable DNS and certificate lifecycle controls.
- Keep PAC size reasonable and avoid heavy DNS lookups inside PAC logic.

## 5) DHCP server examples

The exact syntax varies by DHCP implementation. The examples below show the common pattern: publish PAC URL via option 252.

## 5.1 Kea DHCP (JSON)

```json
{
  "Dhcp4": {
    "option-data": [
      {
        "name": "wpad",
        "code": 252,
        "space": "dhcp4",
        "csv-format": true,
        "data": "https://proxy-config.corp.example/proxy.pac"
      }
    ]
  }
}
```

If your Kea schema/version does not define option name `wpad`, use explicit custom option definition for code `252` in your option space.

## 5.2 ISC dhcpd

```conf
option local-proxy-config code 252 = text;

subnet 192.168.1.0 netmask 255.255.255.0 {
  range 192.168.1.100 192.168.1.200;
  option routers 192.168.1.1;
  option domain-name-servers 192.168.1.10;
  option local-proxy-config "https://proxy-config.corp.example/proxy.pac";
}
```

## 5.3 Windows DHCP Server (PowerShell)

```powershell
Add-DhcpServerv4OptionDefinition -ComputerName dhcp01 \
  -Name "WPAD/PAC URL" -OptionId 252 -Type String

Set-DhcpServerv4OptionValue -ComputerName dhcp01 \
  -ScopeId 192.168.1.0 -OptionId 252 \
  -Value "https://proxy-config.corp.example/proxy.pac"
```

## 5.4 dnsmasq

```conf
dhcp-option=252,"https://proxy-config.corp.example/proxy.pac"
```

## 5.5 "local-pac-server" vendor option name

Some DHCP/network stacks expose a named setting such as `local-pac-server` instead of option-code language.

Mapping guideline:
- If the product maps `local-pac-server` to option 252 PAC URL semantics, set it to your PAC URL.
- If the product uses a vendor-private option code, verify endpoint support before rollout.

## 6) Security and hardening

Use these controls for production:

- Host PAC on trusted internal infrastructure with TLS.
- Restrict who can edit PAC content (change control + code review).
- Avoid unauthenticated/public WPAD exposure across untrusted VLANs.
- Keep `OD_SQUID_ALLOWED_CLIENT_CIDRS` strict in Linux production-like deployments.
- Keep `OD_TRUST_PROXY_HEADERS=false` unless you explicitly trust ingress CIDRs and overwrite headers at the edge.

## 7) Validation checklist

## 7.1 Client-side validation

- Renew DHCP lease and confirm PAC URL is present.
- Verify browser/system proxy mode is set to auto-detect or PAC URL mode as required by your endpoint policy.
- Confirm requests route via Open Defender proxy endpoint.

## 7.2 Proxy-path validation

Run from Open Defender host:

```bash
docker compose --env-file .env -f deploy/docker/docker-compose.yml logs --tail=100 haproxy
docker compose --env-file .env -f deploy/docker/docker-compose.yml logs --tail=100 squid
```

Expected outcome:
- HAProxy accepts client and forwards to Squid backend.
- Squid logs request records and ICAP path continues normally.

## 8) Common failure modes

1. Clients ignore DHCP option 252.
- Cause: endpoint stack does not consume DHCP WPAD.
- Action: use DNS WPAD and/or endpoint management policy (GPO/MDM) for PAC URL.

2. PAC URL is reachable but no traffic goes through proxy.
- Cause: browser/profile set to manual/direct mode.
- Action: enforce auto-proxy mode per platform policy.

3. Proxy returns `403` for valid clients.
- Cause: source CIDR mismatch at HAProxy/Squid ACL layer.
- Action: update `OD_SQUID_ALLOWED_CLIENT_CIDRS`; account for Docker Desktop/macOS source-IP behavior in local dev.

4. Internal apps break after PAC rollout.
- Cause: missing `DIRECT` bypass for internal names/subnets.
- Action: add narrowly scoped bypass rules and retest.

## 9) Rollout strategy

Recommended staged rollout:

1. Pilot VLAN or device group.
2. Validate request path, policy outcomes, and telemetry continuity.
3. Expand DHCP scope gradually.
4. Keep rollback path ready (remove DHCP option / restore prior PAC URL).

This approach minimizes outage risk while preserving policy enforcement and visibility.
