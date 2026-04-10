import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
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
import { useOpsStatus } from '../hooks/useOpsStatus';
import { useDashboardReportData } from '../hooks/useDashboardReportData';

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

export const DashboardPage = () => {
  const navigate = useNavigate();
  const [range, setRange] = useState('24h');
  const [topN, setTopN] = useState(10);
  const [refreshIntervalMs, setRefreshIntervalMs] = useState<number>(() => {
    if (typeof window === 'undefined') return 30_000;
    const raw = window.localStorage.getItem('od.dashboard.refresh.ms');
    const parsed = raw ? Number(raw) : NaN;
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : 30_000;
  });
  const { data: ops, loading: opsLoading, error: opsError, updatedAt: opsUpdatedAt } = useOpsStatus(refreshIntervalMs);
  const dashboard = useDashboardReportData(range, topN, undefined, refreshIntervalMs);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    window.localStorage.setItem('od.dashboard.refresh.ms', String(refreshIntervalMs));
  }, [refreshIntervalMs]);

  const overview = dashboard.data?.overview;
  const coverage = dashboard.data?.coverage;
  const hourlyChart = useMemo(
    () =>
      (dashboard.data?.hourly_usage ?? []).map((entry) => ({
        label: entry.timestamp.slice(11, 16),
        requests: entry.total_requests,
        blocked: entry.blocked_requests,
        bandwidthMiB: Number((entry.bandwidth_bytes / (1024 * 1024)).toFixed(3)),
      })),
    [dashboard.data?.hourly_usage],
  );

  const domainChart = useMemo(
    () =>
      (dashboard.data?.top_domains ?? []).slice(0, 10).map((entry) => ({
        domain: entry.key,
        hits: entry.doc_count,
      })),
    [dashboard.data?.top_domains],
  );

  const blockedDomainChart = useMemo(
    () =>
      (dashboard.data?.top_blocked_domains ?? []).slice(0, 10).map((entry) => ({
        domain: entry.key,
        blocked: entry.doc_count,
      })),
    [dashboard.data?.top_blocked_domains],
  );

  const lastUpdated = Math.max(dashboard.updatedAt ?? 0, opsUpdatedAt ?? 0);
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

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Command Deck</p>
          <h2 style={{ margin: 0, fontSize: '2.4rem' }}>Trust & Safety Pulse</h2>
          <p style={{ color: 'var(--muted)' }}>
            Client-IP traffic intelligence with usage, bandwidth, and block trends.
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
          <select
            className="search-input dashboard-header-select"
            value={refreshIntervalMs}
            onChange={(event) => setRefreshIntervalMs(Number(event.target.value))}
            title="Auto refresh interval"
          >
            <option value={0}>Auto Refresh: Off</option>
            <option value={15000}>Auto Refresh: 15s</option>
            <option value={30000}>Auto Refresh: 30s</option>
            <option value={60000}>Auto Refresh: 60s</option>
          </select>
          <button className="cta-button" onClick={() => dashboard.refresh()} disabled={dashboard.loading}>
            {dashboard.loading ? 'Refreshing...' : 'Refresh'}
          </button>
          <button className="cta-button" onClick={() => navigate('/reports')}>Open Reports</button>
        </div>
      </div>

      <p style={{ marginTop: '-0.9rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
        Last updated: {lastUpdated > 0 ? new Date(lastUpdated).toLocaleTimeString() : '—'}
      </p>

      {dashboard.error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load dashboard analytics: {dashboard.error}</p>
        </div>
      ) : null}

      <div className="kpi-grid">
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'rgba(255,255,255,0.7)' }}>Unique Clients</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {overview ? formatCompact(overview.unique_clients) : '—'}
          </h3>
          <span className="chip chip--green">by client.ip ({range})</span>
        </div>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'rgba(255,255,255,0.7)' }}>Total Bandwidth</p>
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
          <p className="section-title" style={{ color: 'rgba(255,255,255,0.7)' }}>Blocked Requests</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {overview ? formatCompact(overview.blocked_requests) : '—'}
          </h3>
          <span className="chip chip--red">{overview ? pct(overview.blocked_requests, overview.total_requests) : '0.0%'}</span>
        </div>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'rgba(255,255,255,0.7)' }}>Total Requests</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {overview ? formatCompact(overview.total_requests) : '—'}
          </h3>
          <span className="chip chip--green">allow {overview ? formatCompact(overview.allow_requests) : '0'}</span>
        </div>
        <div className="kpi-card">
          <p className="section-title" style={{ color: 'rgba(255,255,255,0.7)' }}>LLM Worker Status</p>
          <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>
            {opsLoading ? '…' : formatCompact(ops.pendingCount)}
          </h3>
          <span className="chip chip--amber">
            providers {opsLoading ? '…' : formatCompact(ops.llmProviderNames.length)}
          </span>
          <p style={{ margin: '0.55rem 0 0', color: 'var(--muted)', fontSize: '0.82rem' }}>
            {opsLoading
              ? 'Loading worker snapshot…'
              : ops.llmProviderNames.length > 0
                ? ops.llmProviderNames.join(', ')
                : 'No provider metadata'}
          </p>
          {opsError ? <p style={{ color: '#ff9b9b', margin: '0.45rem 0 0', fontSize: '0.8rem' }}>{opsError}</p> : null}
          {!opsLoading ? (
            <p style={{ margin: '0.45rem 0 0' }}>
              <span className={`chip chip--${ops.source === 'live' ? 'green' : 'amber'}`}>ops source: {ops.source}</span>
            </p>
          ) : null}
        </div>
      </div>

      <div className="layout-grid" style={{ marginTop: '2rem' }}>
        <div className="glass-panel panel--full">
          <p className="section-title">Hourly Usage (Requests + Bandwidth)</p>
          {bandwidthCoverageGap ? (
            <p style={{ margin: '0 0 0.8rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
              Some records in this range do not include `network.bytes`, so bandwidth totals may appear lower than request volume.
            </p>
          ) : null}
          <div style={{ width: '100%', height: 320 }}>
            <ResponsiveContainer>
              <ComposedChart data={hourlyChart}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.12)" />
                <XAxis dataKey="label" stroke="rgba(255,255,255,0.8)" />
                <YAxis yAxisId="req" stroke="rgba(255,255,255,0.8)" />
                <YAxis yAxisId="bw" orientation="right" stroke="rgba(255,255,255,0.8)" />
                <Tooltip
                  formatter={(value, name) => {
                    if (name === 'Bandwidth (MiB)') {
                      return [formatMiB(Number(value)), name];
                    }
                    return [formatCompact(Number(value)), name];
                  }}
                />
                <Legend />
                <Line name="Requests" yAxisId="req" type="monotone" dataKey="requests" stroke="#7dd3fc" dot={false} />
                <Line name="Blocked" yAxisId="req" type="monotone" dataKey="blocked" stroke="#f87171" dot={false} />
                <Area
                  name="Bandwidth (MiB)"
                  yAxisId="bw"
                  type="monotone"
                  dataKey="bandwidthMiB"
                  stroke="#34d399"
                  fill="#34d39933"
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

      <div className="layout-grid" style={{ marginTop: '1.2rem' }}>
        <div className="glass-panel">
          <p className="section-title">Frequently Accessed Domains</p>
          <div style={{ width: '100%', height: 300 }}>
            <ResponsiveContainer>
              <BarChart data={domainChart} margin={{ left: 16, right: 16 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.12)" />
                <XAxis dataKey="domain" hide />
                <YAxis stroke="rgba(255,255,255,0.8)" />
                <Tooltip />
                <Bar dataKey="hits" fill="#60a5fa" />
              </BarChart>
            </ResponsiveContainer>
          </div>
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Top frequently accessed domains">
            <table>
              <thead>
                <tr>
                  <th>Domain</th>
                  <th>Hits</th>
                </tr>
              </thead>
              <tbody>
                {(dashboard.data?.top_domains ?? []).map((row) => (
                  <tr key={row.key}>
                    <td>{row.key}</td>
                    <td>{formatCompact(row.doc_count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div className="glass-panel">
          <p className="section-title">Blocked Domains</p>
          <div style={{ width: '100%', height: 300 }}>
            <ResponsiveContainer>
              <BarChart data={blockedDomainChart} margin={{ left: 16, right: 16 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.12)" />
                <XAxis dataKey="domain" hide />
                <YAxis stroke="rgba(255,255,255,0.8)" />
                <Tooltip />
                <Bar dataKey="blocked" fill="#f87171" />
              </BarChart>
            </ResponsiveContainer>
          </div>
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Top blocked domains">
            <table>
              <thead>
                <tr>
                  <th>Domain</th>
                  <th>Blocked Hits</th>
                </tr>
              </thead>
              <tbody>
                {(dashboard.data?.top_blocked_domains ?? []).map((row) => (
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
        <div className="glass-panel">
          <p className="section-title">Top Requesters of Blocked Domains (client.ip)</p>
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Top blocked requesters by client ip">
            <table>
              <thead>
                <tr>
                  <th>Client IP</th>
                  <th>Blocked Requests</th>
                </tr>
              </thead>
              <tbody>
                {(dashboard.data?.top_blocked_requesters ?? []).map((row) => (
                  <tr key={row.key}>
                    <td>{row.key}</td>
                    <td>{formatCompact(row.doc_count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div className="glass-panel">
          <p className="section-title">Top Clients by Bandwidth</p>
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Top clients by bandwidth table">
            <table>
              <thead>
                <tr>
                  <th>Client IP</th>
                  <th>Requests</th>
                  <th>Bandwidth</th>
                </tr>
              </thead>
              <tbody>
                {(dashboard.data?.top_clients_by_bandwidth ?? []).map((row) => (
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
        {coverage ? (
          <div className="chip-row">
            <span className="chip chip--green">Client IP coverage: {pct(coverage.client_ip_docs, coverage.total_docs)}</span>
            <span className="chip chip--amber">Domain coverage: {pct(coverage.domain_docs, coverage.total_docs)}</span>
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
