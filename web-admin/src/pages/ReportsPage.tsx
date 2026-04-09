import { useMemo, useState } from 'react';
import { useReportingStatus } from '../hooks/useReportingStatus';
import { useTrafficReportData } from '../hooks/useTrafficReportData';

const toCsvValue = (value: string | number) => {
  const text = String(value);
  if (text.includes(',') || text.includes('"') || text.includes('\n')) {
    return `"${text.replaceAll('"', '""')}"`;
  }
  return text;
};

const downloadCsv = (filename: string, rows: Array<Array<string | number>>) => {
  const csv = rows.map((row) => row.map(toCsvValue).join(',')).join('\n');
  const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
};

const formatCompact = (input: number) => {
  if (!Number.isFinite(input)) return '0';
  return new Intl.NumberFormat('en', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(input);
};

export const ReportsPage = () => {
  const [range, setRange] = useState('24h');
  const [topN, setTopN] = useState(10);

  const traffic = useTrafficReportData(range, topN);
  const reportingStatus = useReportingStatus(range);

  const trendRows = useMemo(() => {
    if (!traffic.data) return [] as { action: string; buckets: number; total: number }[];
    return traffic.data.allow_block_trend.map((series) => ({
      action: series.action,
      buckets: series.buckets.length,
      total: series.buckets.reduce((sum, item) => sum + item.doc_count, 0),
    }));
  }, [traffic.data]);

  const hasTrafficData =
    Boolean(traffic.data) &&
    ((traffic.data?.allow_block_trend.length ?? 0) > 0 ||
      (traffic.data?.top_blocked_domains.length ?? 0) > 0 ||
      (traffic.data?.top_categories.length ?? 0) > 0);

  const exportCsv = () => {
    const rows: Array<Array<string | number>> = [
      ['section', 'key', 'value', 'meta'],
    ];

    const trafficData = traffic.data;
    if (trafficData) {
      trendRows.forEach((row) => {
        rows.push(['traffic-trend', row.action, row.total, `${row.buckets} buckets`]);
      });
      trafficData.top_blocked_domains.forEach((row) => {
        rows.push(['top-blocked-domain', row.key, row.doc_count, trafficData.range]);
      });
      trafficData.top_categories.forEach((row) => {
        rows.push(['top-category', row.key, row.doc_count, trafficData.range]);
      });
    }

    downloadCsv(`open-defender-reports-${range}.csv`, rows);
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Reporting</p>
          <h2 style={{ margin: 0 }}>Traffic Analytics</h2>
        </div>
        <button className="cta-button" onClick={exportCsv}>Export CSV</button>
      </div>

      <div className="glass-panel">
        <p className="section-title">Filters</p>
        <div className="layout-grid">
          <label>
            <span style={{ display: 'block', marginBottom: '0.35rem' }}>Traffic Range</span>
            <select className="search-input" value={range} onChange={(event) => setRange(event.target.value)}>
              <option value="1h">1h</option>
              <option value="6h">6h</option>
              <option value="24h">24h</option>
              <option value="7d">7d</option>
            </select>
          </label>
          <label>
            <span style={{ display: 'block', marginBottom: '0.35rem' }}>Top N</span>
            <select
              className="search-input"
              value={topN}
              onChange={(event) => setTopN(Number(event.target.value))}
            >
              <option value={5}>5</option>
              <option value={10}>10</option>
              <option value={20}>20</option>
            </select>
          </label>
        </div>
      </div>

      {reportingStatus.data ? (
        <div className="glass-panel" style={{ marginTop: '1rem' }}>
          <p className="section-title">Data Quality ({reportingStatus.data.range})</p>
          <div className="layout-grid">
            <div className="kpi-card">
              <p className="section-title">Total docs</p>
              <h3 style={{ margin: 0 }}>{formatCompact(reportingStatus.data.total_docs)}</h3>
            </div>
            <div className="kpi-card">
              <p className="section-title">Action coverage</p>
              <h3 style={{ margin: 0 }}>{formatCompact(reportingStatus.data.action_docs)}</h3>
            </div>
            <div className="kpi-card">
              <p className="section-title">Category coverage</p>
              <h3 style={{ margin: 0 }}>{formatCompact(reportingStatus.data.category_docs)}</h3>
            </div>
            <div className="kpi-card">
              <p className="section-title">Domain coverage</p>
              <h3 style={{ margin: 0 }}>{formatCompact(reportingStatus.data.domain_docs)}</h3>
            </div>
          </div>
        </div>
      ) : null}

      <div className="glass-panel">
        <p className="section-title">Traffic Summary</p>
        {traffic.error ? (
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load traffic summary: {traffic.error}</p>
        ) : traffic.loading ? (
          <div>
            {Array.from({ length: 3 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : traffic.data ? (
          <>
            <p style={{ color: 'var(--muted)' }}>
              Range: {traffic.data.range}, Bucket: {traffic.data.bucket_interval}
            </p>
            {!hasTrafficData ? (
              <div className="card">
                <p className="section-title">No Reportable Traffic Fields</p>
                <p style={{ margin: 0, color: 'var(--muted)' }}>
                  Traffic exists but report fields are sparse for this range. Ensure ingest enrichment is active and wait for fresh events.
                </p>
              </div>
            ) : null}

            <div className="layout-grid" style={{ marginBottom: '1rem' }}>
              {trendRows.map((row) => (
                <div key={row.action} className="kpi-card">
                  <p className="section-title">{row.action}</p>
                  <h3 style={{ margin: '0 0 0.35rem' }}>{formatCompact(row.total)}</h3>
                  <span className="chip chip--amber">{row.buckets} buckets</span>
                </div>
              ))}
            </div>

            <div className="layout-grid">
              <div className="card">
                <p className="section-title">Top Blocked Domains</p>
                <div className="table-wrapper" role="region" tabIndex={0} aria-label="Top blocked domains table">
                  <table>
                    <thead>
                      <tr>
                        <th>Domain</th>
                        <th>Hits</th>
                      </tr>
                    </thead>
                    <tbody>
                      {traffic.data.top_blocked_domains.map((row) => (
                        <tr key={row.key}>
                          <td>{row.key}</td>
                          <td>{formatCompact(row.doc_count)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
              <div className="card">
                <p className="section-title">Top Categories</p>
                <div className="table-wrapper" role="region" tabIndex={0} aria-label="Top categories table">
                  <table>
                    <thead>
                      <tr>
                        <th>Category</th>
                        <th>Hits</th>
                      </tr>
                    </thead>
                    <tbody>
                      {traffic.data.top_categories.map((row) => (
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
          </>
        ) : (
          <p style={{ margin: 0, color: 'var(--muted)' }}>Traffic data is unavailable for this environment.</p>
        )}
      </div>
    </div>
  );
};
