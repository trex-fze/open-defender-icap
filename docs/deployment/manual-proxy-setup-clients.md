# Manual Proxy Setup for Clients (Linux, macOS, Windows)

This guide gives quick manual steps to point endpoint traffic to Open Defender proxy.

Use this when DHCP/PAC auto-configuration is not yet deployed or when validating a pilot device.

## 1) Prerequisites

Before configuring clients, confirm:

- Proxy endpoint host/IP is reachable from the client.
- Proxy port is open (usually `3128`, from `OD_HAPROXY_BIND_PORT`).
- Client source IP is allowed by proxy ACL policy (`OD_SQUID_ALLOWED_CLIENT_CIDRS`).

Typical proxy value:

- host: `<proxy-host-or-ip>`
- port: `3128`

## 2) Windows

## 2.1 GUI method (recommended)

1. Open `Settings -> Network & Internet -> Proxy`.
2. Under `Manual proxy setup`, enable `Use a proxy server`.
3. Enter:
   - Address: `<proxy-host-or-ip>`
   - Port: `3128`
4. Save and test browsing.

## 2.2 WinHTTP CLI (service/system contexts)

```powershell
netsh winhttp set proxy "<proxy-host-or-ip>:3128"
```

Reset:

```powershell
netsh winhttp reset proxy
```

## 3) macOS

## 3.1 GUI method

1. Open `System Settings -> Network`.
2. Select active network interface (`Wi-Fi` or `Ethernet`).
3. Open `Details -> Proxies`.
4. Enable `Web Proxy (HTTP)` and `Secure Web Proxy (HTTPS)`.
5. Enter proxy server `<proxy-host-or-ip>` and port `3128`.
6. Apply and test browsing.

## 3.2 CLI method (`networksetup`)

Replace `Wi-Fi` with your interface name.

```bash
networksetup -setwebproxy "Wi-Fi" <proxy-host-or-ip> 3128
networksetup -setsecurewebproxy "Wi-Fi" <proxy-host-or-ip> 3128
networksetup -setwebproxystate "Wi-Fi" on
networksetup -setsecurewebproxystate "Wi-Fi" on
```

Disable:

```bash
networksetup -setwebproxystate "Wi-Fi" off
networksetup -setsecurewebproxystate "Wi-Fi" off
```

## 4) Linux

Linux setup depends on desktop and application stack. Use one of these methods.

## 4.1 GNOME desktop (common)

1. Open `Settings -> Network -> Network Proxy`.
2. Set mode to `Manual`.
3. Set HTTP/HTTPS proxy to `<proxy-host-or-ip>:3128`.
4. Apply and test browser behavior.

## 4.2 Environment variables (CLI and many apps)

```bash
export http_proxy="http://<proxy-host-or-ip>:3128"
export https_proxy="http://<proxy-host-or-ip>:3128"
export no_proxy="localhost,127.0.0.1,.corp.example"
```

Persist in shell profile as needed.

Unset:

```bash
unset http_proxy https_proxy no_proxy
```

## 5) Quick verification

Use `curl` through proxy:

```bash
curl -I -x http://<proxy-host-or-ip>:3128 http://example.com
curl -I -x http://<proxy-host-or-ip>:3128 https://example.com
```

If requests fail, validate Open Defender proxy logs:

```bash
docker compose --env-file .env -f deploy/docker/docker-compose.yml logs --tail=100 haproxy
docker compose --env-file .env -f deploy/docker/docker-compose.yml logs --tail=100 squid
```

## 6) Common issues

1. `403` from proxy for valid users/devices.
- Likely ACL mismatch; confirm client IP is inside `OD_SQUID_ALLOWED_CLIENT_CIDRS`.

2. Works in browser but not CLI tools.
- Browser uses OS proxy settings; CLI app may require `http_proxy/https_proxy`.

3. macOS Docker Desktop local testing shows unexpected ACL denies.
- Source IP seen in container can differ from LAN client IP in local dev profiles.

## 7) Next step for scale

For fleet deployments, prefer central auto-configuration:

- `docs/deployment/dhcp-proxy-auto-configuration.md`
