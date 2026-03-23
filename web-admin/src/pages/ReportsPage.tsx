import { useReportsData } from '../hooks/useReportsData';

export const ReportsPage = () => {
  const { data, loading, error, isMock } = useReportsData();
  const [report] = data;
  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Reporting</p>
          <h2 style={{ margin: 0 }}>Aggregates & KPIs</h2>
        </div>
        <div style={{ display: 'flex', gap: '0.75rem' }}>
          <button className="cta-button">Export CSV</button>
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
    </div>
  );
};
