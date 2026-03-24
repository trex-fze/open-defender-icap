import { kpis, reviewQueue } from '../data/mockData';
import { useOpsStatus } from '../hooks/useOpsStatus';

export const DashboardPage = () => {
  const { data: ops, loading: opsLoading, error: opsError } = useOpsStatus();

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Command Deck</p>
          <h2 style={{ margin: 0, fontSize: '2.4rem' }}>Trust & Safety Pulse</h2>
          <p style={{ color: 'var(--muted)' }}>Live telemetry from ICAP adaptor, Redis queue, and LLM worker.</p>
        </div>
        <button className="cta-button">Download Report</button>
      </div>

      <div className="kpi-grid">
        {kpis.map((kpi) => (
          <div key={kpi.label} className="kpi-card">
            <p className="section-title" style={{ color: 'rgba(255,255,255,0.6)' }}>
              {kpi.label}
            </p>
            <h3 style={{ margin: '0 0 0.4rem', fontSize: '2rem' }}>{kpi.value}</h3>
            <span className={`chip chip--${kpi.tone}`}>{kpi.change}</span>
          </div>
        ))}
      </div>

      <div className="layout-grid" style={{ marginTop: '2rem' }}>
        <div className="glass-panel">
          <p className="section-title">LLM Worker Status</p>
          {opsLoading ? (
            <>
              <div className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
              <div className="skeleton" style={{ width: '70%' }}></div>
            </>
          ) : (
            <>
              <p style={{ fontSize: '1.1rem', margin: '0 0 0.6rem' }}>
                Pending classifications: {ops.pendingCount.toLocaleString()}
              </p>
              <p style={{ fontSize: '1.1rem', margin: '0 0 0.6rem' }}>
                Review queue depth: {ops.reviewQueueCount.toLocaleString()}
              </p>
              <p style={{ margin: 0, color: 'var(--muted)' }}>
                Providers:{' '}
                {ops.llmProviderNames.length > 0 ? ops.llmProviderNames.join(', ') : 'not available in this environment'}
              </p>
              <p style={{ marginTop: '0.6rem' }}>
                <span className={`chip chip--${ops.source === 'live' ? 'green' : 'amber'}`}>ops source: {ops.source}</span>
              </p>
            </>
          )}
          {opsError ? <p style={{ color: '#ff9b9b', marginBottom: 0 }}>Ops status warning: {opsError}</p> : null}
        </div>
        <div className="glass-panel">
          <p className="section-title">Review SLA</p>
        <div className="table-wrapper" role="region" tabIndex={0} aria-label="Review SLA table">
            <table>
              <thead>
                <tr>
                  <th>Item</th>
                  <th>Status</th>
                  <th>SLA</th>
                </tr>
              </thead>
              <tbody>
                {reviewQueue.slice(0, 3).map((item) => (
                  <tr key={item.id}>
                    <td>{item.key}</td>
                    <td>
                      <span className={`chip chip--${item.risk === 'critical' ? 'red' : 'amber'}`}>{item.status}</span>
                    </td>
                    <td>{item.sla}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
};
