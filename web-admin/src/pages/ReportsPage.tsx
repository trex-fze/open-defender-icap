import { reports } from '../data/mockData';

export const ReportsPage = () => {
  const [report] = reports;
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

      <div className="glass-panel">
        <p className="section-title">Dimension: {report.dimension}</p>
        <div className="layout-grid">
          {Object.entries(report.metrics).map(([action, value]) => (
            <div key={action} className="kpi-card">
              <p className="section-title">{action}</p>
              <h3 style={{ margin: 0 }}>{value.toLocaleString()}</h3>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
