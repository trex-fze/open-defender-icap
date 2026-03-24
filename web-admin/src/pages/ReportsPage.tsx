import { useMemo, useState } from 'react';
import { useReportsData } from '../hooks/useReportsData';
import { useTrafficReportData } from '../hooks/useTrafficReportData';

export const ReportsPage = () => {
  const [dimension, setDimension] = useState('category');
  const [range, setRange] = useState('24h');
  const [topN, setTopN] = useState(10);

  const { data, loading, error, isMock } = useReportsData(dimension);
  const traffic = useTrafficReportData(range, topN);
  const [report] = data;

  const trendRows = useMemo(() => {
    if (!traffic.data) return [] as { action: string; buckets: number; total: number }[];
    return traffic.data.allow_block_trend.map((series) => ({
      action: series.action,
      buckets: series.buckets.length,
      total: series.buckets.reduce((sum, item) => sum + item.doc_count, 0),
    }));
  }, [traffic.data]);

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Reporting</p>
          <h2 style={{ margin: 0 }}>Aggregates & KPIs</h2>
        </div>
        <button className="cta-button">Export CSV</button>
      </div>

      <div className="glass-panel">
        <p className="section-title">Filters</p>
        <div className="layout-grid">
          <label>
            <span style={{ display: 'block', marginBottom: '0.35rem' }}>Aggregate Dimension</span>
            <select className="search-input" value={dimension} onChange={(event) => setDimension(event.target.value)}>
              <option value="category">category</option>
              <option value="action">action</option>
              <option value="risk">risk</option>
            </select>
          </label>
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

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load aggregates: {error}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: '#fdd744', marginTop: '0.5rem' }}>
          Mock stream (Admin API offline)
        </p>
      ) : null}

      <div className="glass-panel">
        {loading || !report ? (
          <div>
            {Array.from({ length: 4 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : (
          <>
            <p className="section-title">Dimension: {report.dimension}</p>
            <div className="layout-grid" style={{ marginBottom: '1.5rem' }}>
              {Object.entries(report.metrics).map(([action, value]) => (
                <div key={action} className="kpi-card">
                  <p className="section-title">{action}</p>
                  <h3 style={{ margin: 0 }}>{Number(value).toLocaleString()}</h3>
                </div>
              ))}
            </div>
            <div className="table-wrapper" role="region" tabIndex={0} aria-label="Reports table">
              <table>
                <thead>
                  <tr>
                    <th>Dimension</th>
                    <th>Period</th>
                    <th>Created</th>
                  </tr>
                </thead>
                <tbody>
                  {data.map((item) => (
                    <tr key={item.id}>
                      <td>{item.dimension}</td>
                      <td>{item.period}</td>
                      <td>{new Date(item.createdAt).toLocaleString()}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </div>

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
            <div className="layout-grid" style={{ marginBottom: '1rem' }}>
              {trendRows.map((row) => (
                <div key={row.action} className="kpi-card">
                  <p className="section-title">{row.action}</p>
                  <h3 style={{ margin: '0 0 0.35rem' }}>{row.total.toLocaleString()}</h3>
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
                          <td>{row.doc_count.toLocaleString()}</td>
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
                          <td>{row.doc_count.toLocaleString()}</td>
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
