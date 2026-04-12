import { useEffect, useMemo, useState } from 'react';
import {
  ResponsiveContainer,
  Area,
  Bar,
  BarChart,
  CartesianGrid,
  ComposedChart,
  Legend,
  Line,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { useOpsStatus, type OpsProviderStatus } from '../hooks/useOpsStatus';
import { useDashboardReportData } from '../hooks/useDashboardReportData';
import { useDashboardOpsSummary } from '../hooks/useDashboardOpsSummary';
import { useTheme } from '../context/ThemeContext';

const formatBytes = (input: number) => {
  if (!Number.isFinite(input) || input <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = input;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2)} ${units[idx]}`;
};

const formatCompact = (input: number) => {
  if (!Number.isFinite(input)) return '0';
  return new Intl.NumberFormat('en', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(input);
};

const pct = (num: number, den: number) => {
  if (!den) return '0.0%';
  return `${((num / den) * 100).toFixed(1)}%`;
};

const formatMiB = (input: number) => {
  if (!Number.isFinite(input) || input <= 0) return '0.000 MiB';
  return `${input.toFixed(3)} MiB`;
};

const formatBucketLabel = (timestamp: string, timezone?: string) => {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return timestamp;
  return new Intl.DateTimeFormat('en-GB', {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
    timeZone: timezone,
  }).format(date);
};

const formatRate = (value?: number) => {
  if (value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(3)}/s`;
};

const hasNumber = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);

type ChartPalette = {
  grid: string;
  axis: string;
  requests: string;
  blocked: string;
  bandwidthStroke: string;
  bandwidthFill: string;
  tooltipBg: string;
  tooltipBorder: string;
  tooltipTitle: string;
  tooltipText: string;
};

const CHART_PALETTE_DARK: ChartPalette = {
  grid: 'rgba(255,255,255,0.08)',
  axis: 'rgba(255,255,255,0.78)',
  requests: '#7aa8ff',
  blocked: '#f87171',
  bandwidthStroke: '#5bc6ae',
  bandwidthFill: '#5bc6ae33',
  tooltipBg: 'rgba(6, 12, 24, 0.95)',
  tooltipBorder: 'rgba(255,255,255,0.16)',
  tooltipTitle: '#f8fbff',
  tooltipText: '#c8d6ff',
};

const CHART_PALETTE_LIGHT: ChartPalette = {
  grid: 'rgba(27,44,68,0.16)',
  axis: 'rgba(27,44,68,0.86)',
  requests: '#356fd4',
  blocked: '#cd4e66',
  bandwidthStroke: '#188b72',
  bandwidthFill: '#188b7230',
  tooltipBg: 'rgba(248, 251, 255, 0.97)',
  tooltipBorder: 'rgba(31, 57, 87, 0.25)',
  tooltipTitle: '#0e1b2b',
  tooltipText: '#46566f',
};

const renderDomainTooltip = (valueLabel: string, palette: ChartPalette) => ({ active, payload }: { active?: boolean; payload?: Array<{ value?: number; payload?: { domain?: string } }> }) => {
  if (!active || !payload || payload.length === 0) return null;
  const point = payload[0];
  const domain = point?.payload?.domain?.trim() || '(unknown domain)';
  return (
    <div
      style={{
        background: palette.tooltipBg,
        border: `1px solid ${palette.tooltipBorder}`,
        borderRadius: '0.75rem',
        padding: '0.6rem 0.75rem',
      }}
    >
      <p style={{ margin: '0 0 0.3rem', color: palette.tooltipTitle, fontWeight: 600 }}>Domain: {domain}</p>
      <p style={{ margin: 0, color: palette.tooltipText }}>
        {valueLabel}: {formatCompact(Number(point?.value ?? 0))}
      </p>
    </div>
  );
};

const providerHealthChipClass = (provider: OpsProviderStatus) => {
  switch (provider.healthStatus) {
    case 'healthy':
      return 'chip--green';
    case 'degraded':
    case 'unknown':
      return 'chip--amber';
    case 'unreachable':
    case 'misconfigured':
      return 'chip--red';
    default:
      return 'chip--amber';
  }
};

export const DashboardPage = () => {
  const { resolvedTheme } = useTheme();
  const [range, setRange] = useState('1h');
  const [topN, setTopN] = useState(20);
  const [refreshIntervalMs, setRefreshIntervalMs] = useState<number>(() => {
    if (typeof window === 'undefined') return 30_000;
    const raw = window.localStorage.getItem('od.dashboard.refresh.ms');
    const parsed = raw ? Number(raw) : NaN;
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : 30_000;
  });
  const { data: ops, loading: opsLoading, error: opsError, updatedAt: opsUpdatedAt } = useOpsStatus(refreshIntervalMs);
  const dashboard = useDashboardReportData(range, topN, undefined, refreshIntervalMs);
  const opsSummary = useDashboardOpsSummary(range, refreshIntervalMs);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    window.localStorage.setItem('od.dashboard.refresh.ms', String(refreshIntervalMs));
  }, [refreshIntervalMs]);

  const overview = dashboard.data?.overview;
  const coverage = dashboard.data?.coverage;
  const chartPalette = useMemo(
    () => (resolvedTheme === 'light' ? CHART_PALETTE_LIGHT : CHART_PALETTE_DARK),
    [resolvedTheme],
  );
  const hourlyChart = useMemo(
    () =>
      (dashboard.data?.hourly_usage ?? []).map((entry) => ({
        timestamp: entry.timestamp,
        requests: entry.total_requests,
        blocked: entry.blocked_requests,
        bandwidthMiB: Number((entry.bandwidth_bytes / (1024 * 1024)).toFixed(3)),
      })),
    [dashboard.data?.hourly_usage],
  );

  const domainChart = useMemo(
    () =>
      (dashboard.data?.top_domains ?? []).slice(0, 10).map((entry) => ({
        domain: entry.key?.trim() || '(unknown domain)',
        hits: entry.doc_count,
      })),
    [dashboard.data?.top_domains],
  );

  const blockedDomainChart = useMemo(
    () =>
      (dashboard.data?.top_blocked_domains ?? []).slice(0, 10).map((entry) => ({
        domain: entry.key?.trim() || '(unknown domain)',
        blocked: entry.doc_count,
      })),
    [dashboard.data?.top_blocked_domains],
  );

  const topDomains = dashboard.data?.top_domains ?? [];
  const topBlockedDomains = dashboard.data?.top_blocked_domains ?? [];
  const topBlockedRequesters = dashboard.data?.top_blocked_requesters ?? [];
  const topClientsByBandwidth = dashboard.data?.top_clients_by_bandwidth ?? [];
  const topCategories = dashboard.data?.top_categories ?? [];
  const topCategoriesEvent = dashboard.data?.top_categories_event ?? [];

  const lastUpdated = Math.max(dashboard.updatedAt ?? 0, opsUpdatedAt ?? 0, opsSummary.updatedAt ?? 0);
  const topClientsBandwidthBytes = useMemo(
    () => (dashboard.data?.top_clients_by_bandwidth ?? []).reduce((sum, row) => sum + row.bandwidth_bytes, 0),
    [dashboard.data?.top_clients_by_bandwidth],
  );
  const hourlyBandwidthBytes = useMemo(
    () => (dashboard.data?.hourly_usage ?? []).reduce((sum, row) => sum + row.bandwidth_bytes, 0),
    [dashboard.data?.hourly_usage],
  );
  const bandwidthCoverageGap =
    coverage && coverage.total_docs > 0 && coverage.network_bytes_docs < coverage.total_docs;
  const clientCoverageGap = coverage && coverage.total_docs > 0 && coverage.client_ip_docs < coverage.total_docs;
  const categoryCoverageGap = coverage && coverage.total_docs > 0 && coverage.category_docs < coverage.total_docs;
  const categoryMappedCoverageGap =
    coverage && coverage.total_docs > 0 && coverage.category_mapped_domain_docs < coverage.total_docs;
  const healthyProviders = useMemo(
    () => ops.llmProviders.filter((provider) => provider.healthStatus === 'healthy').length,
    [ops.llmProviders],
  );
  const unhealthyProviders = useMemo(
    () => ops.llmProviders.filter((provider) => provider.healthStatus !== 'healthy').length,
    [ops.llmProviders],
  );

  return (
    <div>
      <div className="page-header">
        <div>
          <h2 style={{ margin: 0, fontSize: '2.4rem' }}>Dashboard</h2>
          <p style={{ color: 'var(--muted)' }}>
            AI Enhanced Web Security Platform
          </p>
        </div>
        <div className="page-header-actions dashboard-header-actions">
          <select className="search-input dashboard-header-select" value={range} onChange={(event) => setRange(event.target.value)}>
            <option value="1h">1h</option>
            <option value="6h">6h</option>
            <option value="24h">24h</option>
            <option value="7d">7d</option>
            <option value="30d">30d</option>
          </select>
          <select className="search-input dashboard-header-select" value={topN} onChange={(event) => setTopN(Number(event.target.value))}>
            <option value={5}>Top 5</option>
            <option value={10}>Top 10</option>
            <option value={20}>Top 20</option>
          </select>
          <div className="dashboard-refresh-control">
            <span className="dashboard-refresh-label">Auto Refresh:</span>
            <select
              className="search-input dashboard-header-select"
              value={refreshIntervalMs}
              onChange={(event) => setRefreshIntervalMs(Number(event.target.value))}
              title="Auto refresh interval"
            >
              <option value={0}>Off</option>
              <option value={15000}>15s</option>
              <option value={30000}>30s</option>
              <option value={60000}>60s</option>
            </select>
          </div>
          <button className="cta-button" onClick={() => dashboard.refresh()} disabled={dashboard.loading}>
            {dashboard.loading ? 'Refreshing...' : 'Refresh'}
          </button>
        </div>
      </div>

      <p style={{ marginTop: '-0.9rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
        Last updated: {lastUpdated > 0 ? new Date(lastUpdated).toLocaleTimeString() : '—'}
      </p>

      {dashboard.error ? (
        <div className="glass-panel glass-panel--error">
          <p style={{ margin: 0, color: 'var(--status-error)' }}>Failed to load dashboard analytics: {dashboard.error}</p>
        </div>
      ) : null}
      {opsSummary.error ? (
        <div className="glass-panel glass-panel--error" style={{ marginTop: '0.9rem' }}>
          <p style={{ margin: 0, color: 'var(--status-error)' }}>
            Failed to load Prometheus operations telemetry: {opsSummary.error}
          </p>
        </div>
      ) : null}

      <div className="layout-grid" style={{ marginTop: '1.1rem' }}>
        <div className="glass-panel panel--full dashboard-hourly-panel">
          <p className="section-title">Usage (Requests + Bandwidth)</p>
          <p style={{ margin: '0 0 0.55rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
            Bucket: {dashboard.data?.bucket_interval ?? 'auto'} · Time zone: {dashboard.data?.timezone ?? 'browser local'}
          </p>
          {bandwidthCoverageGap ? (
            <p style={{ margin: '0 0 0.8rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
              Some records in this range do not include `network.bytes`, so bandwidth totals may appear lower than request volume.
            </p>
          ) : null}
          <div className="dashboard-hourly-chart">
            <ResponsiveContainer>
              <ComposedChart data={hourlyChart} margin={{ top: 8, right: 0, bottom: 4, left: 0 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={chartPalette.grid} />
                <XAxis
                  dataKey="timestamp"
                  stroke={chartPalette.axis}
                  tick={{ fill: chartPalette.axis, fontSize: 12 }}
                  axisLine={false}
                  tickLine={false}
                  tickFormatter={(value) => formatBucketLabel(String(value), dashboard.data?.timezone)}
                />
                <YAxis
                  yAxisId="req"
                  stroke={chartPalette.axis}
                  tick={{ fill: chartPalette.axis, fontSize: 12 }}
                  axisLine={false}
                  tickLine={false}
                />
                <YAxis
                  yAxisId="bw"
                  orientation="right"
                  stroke={chartPalette.axis}
                  tick={{ fill: chartPalette.axis, fontSize: 12 }}
                  axisLine={false}
                  tickLine={false}
                />
                <Tooltip
                  contentStyle={{
                    background: chartPalette.tooltipBg,
                    border: `1px solid ${chartPalette.tooltipBorder}`,
                    borderRadius: '0.75rem',
                    color: chartPalette.tooltipText,
                  }}
                  labelStyle={{ color: chartPalette.tooltipTitle }}
                  formatter={(value, name) => {
                    if (name === 'Bandwidth (MiB)') {
                      return [formatMiB(Number(value)), name];
                    }
                    return [formatCompact(Number(value)), name];
                  }}
                  labelFormatter={(label) => formatBucketLabel(String(label), dashboard.data?.timezone)}
                />
                <Legend wrapperStyle={{ color: chartPalette.axis }} />
                <Line name="Requests" yAxisId="req" type="monotone" dataKey="requests" stroke={chartPalette.requests} strokeWidth={2.2} dot={false} />
                <Line name="Blocked" yAxisId="req" type="monotone" dataKey="blocked" stroke={chartPalette.blocked} strokeWidth={2.2} dot={false} />
                <Area
                  name="Bandwidth (MiB)"
                  yAxisId="bw"
                  type="monotone"
                  dataKey="bandwidthMiB"
                  stroke={chartPalette.bandwidthStroke}
                  fill={chartPalette.bandwidthFill}
                  fillOpacity={0.26}
                  strokeWidth={2}
                />
              </ComposedChart>
            </ResponsiveContainer>
          </div>
          {overview ? (
            <p style={{ margin: '0.8rem 0 0', color: 'var(--muted)', fontSize: '0.82rem' }}>
              Hourly bucket sum: {formatBytes(hourlyBandwidthBytes)} (overview total: {formatBytes(overview.total_bandwidth_bytes)})
            </p>
          ) : null}
        </div>
      </div>

      <div className="kpi-grid" style={{ marginTop: '1.2rem' }}>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'var(--text-subtle)' }}>Unique Clients</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {overview ? formatCompact(overview.unique_clients) : '—'}
          </h3>
          <span className="chip chip--green">by client.ip ({range})</span>
        </div>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'var(--text-subtle)' }}>Total Bandwidth</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {overview ? formatBytes(overview.total_bandwidth_bytes) : '—'}
          </h3>
          <span className="chip chip--amber">aggregated bytes ({range})</span>
          <p style={{ margin: '0.55rem 0 0', color: 'var(--muted)', fontSize: '0.82rem' }}>
            Summed proxy payload bytes (`network.bytes`) for selected range.
          </p>
          {overview ? (
            <p style={{ margin: '0.45rem 0 0', color: 'var(--muted)', fontSize: '0.82rem' }}>
              Top {topN} clients shown: {formatBytes(topClientsBandwidthBytes)} ({pct(topClientsBandwidthBytes, overview.total_bandwidth_bytes)} of total)
            </p>
          ) : null}
        </div>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'var(--text-subtle)' }}>Blocked Requests</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {overview ? formatCompact(overview.blocked_requests) : '—'}
          </h3>
          <span className="chip chip--red">{overview ? pct(overview.blocked_requests, overview.total_requests) : '0.0%'}</span>
        </div>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'var(--text-subtle)' }}>Total Requests</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {overview ? formatCompact(overview.total_requests) : '—'}
          </h3>
          <span className="chip chip--green">allow {overview ? formatCompact(overview.allow_requests) : '0'}</span>
        </div>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'var(--text-subtle)' }}>LLM Worker Status</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {opsLoading ? '…' : formatCompact(ops.pendingCount)}
          </h3>
          <span className={`chip ${opsLoading || healthyProviders === ops.llmProviders.length ? 'chip--green' : 'chip--amber'}`}>
            healthy {opsLoading ? '…' : `${formatCompact(healthyProviders)}/${formatCompact(ops.llmProviders.length)}`}
          </span>
          {!opsLoading && unhealthyProviders > 0 ? (
            <span className="chip chip--red" style={{ marginLeft: '0.4rem' }}>
              unhealthy {formatCompact(unhealthyProviders)}
            </span>
          ) : null}
          <p style={{ margin: '0.55rem 0 0', color: 'var(--muted)', fontSize: '0.82rem' }}>
            {opsLoading
              ? 'Loading worker snapshot…'
              : ops.llmProviders.length > 0
                ? 'Live provider health from llm-worker probe cache.'
                : 'No provider metadata'}
          </p>
          {!opsLoading && ops.llmProviders.length > 0 ? (
            <div style={{ marginTop: '0.45rem', display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
              {ops.llmProviders.map((provider) => (
                <div key={provider.name} style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', flexWrap: 'wrap' }}>
                  <span className={`chip ${providerHealthChipClass(provider)}`}>
                    {provider.name} ({provider.healthStatus})
                  </span>
                  <span style={{ color: 'var(--muted)', fontSize: '0.78rem' }}>
                    {provider.role} · {provider.providerType}
                    {provider.healthLatencyMs !== undefined ? ` · ${provider.healthLatencyMs}ms` : ''}
                    {provider.healthHttpStatus !== undefined ? ` · HTTP ${provider.healthHttpStatus}` : ''}
                  </span>
                  {provider.healthDetail ? (
                    <span style={{ color: 'var(--status-warning)', fontSize: '0.74rem' }}>{provider.healthDetail}</span>
                  ) : null}
                </div>
              ))}
            </div>
          ) : null}
          {opsError ? <p style={{ color: 'var(--status-error)', margin: '0.45rem 0 0', fontSize: '0.8rem' }}>{opsError}</p> : null}
          {!opsLoading ? (
            <p style={{ margin: '0.45rem 0 0' }}>
              <span className={`chip chip--${ops.source === 'live' ? 'green' : 'amber'}`}>ops source: {ops.source}</span>
            </p>
          ) : null}
        </div>
      </div>

      <div className="glass-panel" style={{ marginTop: '1.2rem' }}>
        <p className="section-title">Operations Telemetry (Prometheus)</p>
        {opsSummary.loading ? (
          <p style={{ margin: 0, color: 'var(--muted)' }}>Loading operations telemetry…</p>
        ) : opsSummary.data ? (
          <>
            <p style={{ marginTop: 0, color: 'var(--muted)', fontSize: '0.82rem' }}>
              Runtime source: {opsSummary.data.source} · range: {opsSummary.data.range}
            </p>
            <div className="chip-row" style={{ marginBottom: '0.7rem' }}>
              <span className="chip chip--amber">
                Pending age p95: {hasNumber(opsSummary.data.queue.pending_age_p95_seconds) ? `${opsSummary.data.queue.pending_age_p95_seconds.toFixed(1)}s` : '—'}
              </span>
              <span className="chip chip--green">LLM started: {formatRate(opsSummary.data.queue.llm_jobs_started_per_sec_10m)}</span>
              <span className="chip chip--green">LLM completed: {formatRate(opsSummary.data.queue.llm_jobs_completed_per_sec_10m)}</span>
              <span className="chip chip--red">
                LLM DLQ +10m: {hasNumber(opsSummary.data.queue.llm_dlq_growth_10m) ? formatCompact(opsSummary.data.queue.llm_dlq_growth_10m) : '—'}
              </span>
              <span className="chip chip--red">
                Fetch DLQ +10m: {hasNumber(opsSummary.data.queue.page_fetch_dlq_growth_10m) ? formatCompact(opsSummary.data.queue.page_fetch_dlq_growth_10m) : '—'}
              </span>
              <span className="chip chip--red">
                Login failures +10m: {hasNumber(opsSummary.data.auth.login_failures_10m) ? formatCompact(opsSummary.data.auth.login_failures_10m) : '—'}
              </span>
              <span className="chip chip--amber">
                Lockouts +10m: {hasNumber(opsSummary.data.auth.lockouts_10m) ? formatCompact(opsSummary.data.auth.lockouts_10m) : '—'}
              </span>
              <span className="chip chip--amber">
                Refresh failures +10m: {hasNumber(opsSummary.data.auth.refresh_failures_10m) ? formatCompact(opsSummary.data.auth.refresh_failures_10m) : '—'}
              </span>
            </div>
            {opsSummary.data.providers.length > 0 ? (
              <div className="table-wrapper dashboard-domain-table" role="region" tabIndex={0} aria-label="Prometheus provider telemetry">
                <table>
                  <thead>
                    <tr>
                      <th>Provider</th>
                      <th>Failures (5m)</th>
                      <th>Timeouts (5m)</th>
                      <th>Latency p95</th>
                    </tr>
                  </thead>
                  <tbody>
                    {opsSummary.data.providers.map((item) => (
                      <tr key={item.provider}>
                        <td>{item.provider}</td>
                        <td>{formatCompact(item.failures_5m)}</td>
                        <td>{formatCompact(item.timeouts_5m)}</td>
                        <td>{hasNumber(item.latency_p95_seconds) ? `${item.latency_p95_seconds.toFixed(2)}s` : '—'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <p style={{ margin: 0, color: 'var(--muted)' }}>No provider telemetry samples available for the selected range.</p>
            )}
            {opsSummary.data.errors.length > 0 ? (
              <p style={{ marginTop: '0.65rem', color: 'var(--status-warning)', fontSize: '0.8rem' }}>
                Partial telemetry: {opsSummary.data.errors[0]}
              </p>
            ) : null}
          </>
        ) : null}
      </div>

      <div className="layout-grid" style={{ marginTop: '1.2rem' }}>
        <div className="glass-panel dashboard-domain-panel">
          <p className="section-title">Frequently Accessed Domains</p>
          <p style={{ margin: '0 0 0.45rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
            Showing {formatCompact(topDomains.length)} of requested Top {formatCompact(topN)}.
          </p>
          <div className="dashboard-domain-chart">
            <ResponsiveContainer>
              <BarChart data={domainChart} margin={{ left: 16, right: 16 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={chartPalette.grid} />
                <XAxis dataKey="domain" hide />
                <YAxis stroke={chartPalette.axis} tick={{ fill: chartPalette.axis, fontSize: 12 }} />
                <Tooltip content={renderDomainTooltip('Hits', chartPalette)} />
                <Bar dataKey="hits" fill={chartPalette.requests} />
              </BarChart>
            </ResponsiveContainer>
          </div>
          <div className="table-wrapper dashboard-domain-table" role="region" tabIndex={0} aria-label="Top frequently accessed domains">
            <table>
              <thead>
                <tr>
                  <th>Domain</th>
                  <th>Hits</th>
                </tr>
              </thead>
              <tbody>
                {topDomains.map((row) => (
                  <tr key={row.key}>
                    <td>{row.key}</td>
                    <td>{formatCompact(row.doc_count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div className="glass-panel dashboard-domain-panel">
          <p className="section-title">Blocked Domains</p>
          <p style={{ margin: '0 0 0.45rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
            Showing {formatCompact(topBlockedDomains.length)} of requested Top {formatCompact(topN)}.
          </p>
          <div className="dashboard-domain-chart">
            <ResponsiveContainer>
              <BarChart data={blockedDomainChart} margin={{ left: 16, right: 16 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={chartPalette.grid} />
                <XAxis dataKey="domain" hide />
                <YAxis stroke={chartPalette.axis} tick={{ fill: chartPalette.axis, fontSize: 12 }} />
                <Tooltip content={renderDomainTooltip('Blocked Hits', chartPalette)} />
                <Bar dataKey="blocked" fill={chartPalette.blocked} />
              </BarChart>
            </ResponsiveContainer>
          </div>
          <div className="table-wrapper dashboard-domain-table" role="region" tabIndex={0} aria-label="Top blocked domains">
            <table>
              <thead>
                <tr>
                  <th>Domain</th>
                  <th>Blocked Hits</th>
                </tr>
              </thead>
              <tbody>
                {topBlockedDomains.map((row) => (
                  <tr key={row.key}>
                    <td>{row.key}</td>
                    <td>{formatCompact(row.doc_count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <div className="layout-grid" style={{ marginTop: '1.2rem' }}>
        <div className="glass-panel dashboard-domain-panel dashboard-domain-panel--compact">
          <p className="section-title">Top Categories</p>
          <p style={{ margin: '0 0 0.45rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
            Showing {formatCompact(topCategories.length)} of requested Top {formatCompact(topN)}.
          </p>
          <p style={{ margin: '0 0 0.6rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
            Source: classification-mapped categories from top domains.
          </p>
          {coverage && coverage.total_docs > 0 && coverage.category_mapped_domain_docs === 0 ? (
            <p style={{ margin: '0 0 0.6rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
              No mapped classifications found for top domains in selected range; values fall back to <code>unknown-unclassified</code>.
            </p>
          ) : null}
          {coverage && coverage.total_docs > 0 && coverage.category_docs === 0 && topCategoriesEvent.length > 0 ? (
            <p style={{ margin: '0 0 0.6rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
              Event stream category field is currently absent; mapped classifications are used for category visibility.
            </p>
          ) : null}
          <div className="table-wrapper dashboard-domain-table" role="region" tabIndex={0} aria-label="Top categories table">
            <table>
              <thead>
                <tr>
                  <th>Category</th>
                  <th>Hits</th>
                </tr>
              </thead>
              <tbody>
                {topCategories.length > 0 ? topCategories.map((row) => (
                  <tr key={row.key}>
                    <td>{row.key}</td>
                    <td>{formatCompact(row.doc_count)}</td>
                  </tr>
                )) : (
                  <tr>
                    <td colSpan={2} style={{ color: 'var(--muted)' }}>
                      No category data for selected range.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>

        <div className="glass-panel dashboard-domain-panel dashboard-domain-panel--compact">
          <p className="section-title">Top Requesters of Blocked Domains (client.ip)</p>
          <p style={{ margin: '0 0 0.45rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
            Showing {formatCompact(topBlockedRequesters.length)} of requested Top {formatCompact(topN)}.
          </p>
          <div className="table-wrapper dashboard-domain-table" role="region" tabIndex={0} aria-label="Top blocked requesters by client ip">
            <table>
              <thead>
                <tr>
                  <th>Client IP</th>
                  <th>Blocked Requests</th>
                </tr>
              </thead>
              <tbody>
                {topBlockedRequesters.map((row) => (
                  <tr key={row.key}>
                    <td>{row.key}</td>
                    <td>{formatCompact(row.doc_count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div className="glass-panel dashboard-domain-panel dashboard-domain-panel--compact">
          <p className="section-title">Top Clients by Bandwidth</p>
          <p style={{ margin: '0 0 0.45rem', color: 'var(--muted)', fontSize: '0.78rem' }}>
            Showing {formatCompact(topClientsByBandwidth.length)} of requested Top {formatCompact(topN)}.
          </p>
          <div className="table-wrapper dashboard-domain-table" role="region" tabIndex={0} aria-label="Top clients by bandwidth table">
            <table>
              <thead>
                <tr>
                  <th>Client IP</th>
                  <th>Requests</th>
                  <th>Bandwidth</th>
                </tr>
              </thead>
              <tbody>
                {topClientsByBandwidth.map((row) => (
                  <tr key={row.key}>
                    <td>{row.key}</td>
                    <td>{formatCompact(row.doc_count)}</td>
                    <td>{formatBytes(row.bandwidth_bytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <div className="glass-panel" style={{ marginTop: '1.2rem' }}>
        <p className="section-title">Data Quality and Coverage</p>
        {clientCoverageGap ? (
          <p style={{ margin: '0 0 0.7rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
            Not all events include `client.ip`; top-clients bandwidth can be lower than total bandwidth.
          </p>
        ) : null}
        {categoryCoverageGap ? (
          <p style={{ margin: '0 0 0.7rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
            Not all events include `category`; event-derived categories may collapse into fallback values.
          </p>
        ) : null}
        {categoryMappedCoverageGap ? (
          <p style={{ margin: '0 0 0.7rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
            Not all events map to active classifications; mapped top-categories include fallback for unmatched domains.
          </p>
        ) : null}
        {coverage ? (
          <div className="chip-row">
            <span className="chip chip--green">Client IP coverage: {pct(coverage.client_ip_docs, coverage.total_docs)}</span>
            <span className="chip chip--amber">Domain coverage: {pct(coverage.domain_docs, coverage.total_docs)}</span>
            <span className="chip chip--amber">Event category coverage: {pct(coverage.category_docs, coverage.total_docs)}</span>
            <span className="chip chip--amber">Mapped category coverage: {pct(coverage.category_mapped_domain_docs, coverage.total_docs)}</span>
            <span className="chip chip--amber">Bandwidth coverage: {pct(coverage.network_bytes_docs, coverage.total_docs)}</span>
            <span className="chip chip--amber">Total docs: {formatCompact(coverage.total_docs)}</span>
          </div>
        ) : (
          <p style={{ margin: 0, color: 'var(--muted)' }}>
            Coverage metrics unavailable in this environment.
          </p>
        )}
        {dashboard.isMock ? (
          <p style={{ marginTop: '0.8rem', color: 'var(--muted)' }}>
            Dashboard is in mock/offline mode because Admin API reporting is unavailable.
          </p>
        ) : null}
      </div>
    </div>
  );
};
