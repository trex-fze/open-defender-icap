import { kpis } from '../data/mockData';
import { useOpsStatus } from '../hooks/useOpsStatus';
import { useNavigate } from 'react-router-dom';

export const DashboardPage = () => {
  const navigate = useNavigate();
  const { data: ops, loading: opsLoading, error: opsError } = useOpsStatus();

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Command Deck</p>
          <h2 style={{ margin: 0, fontSize: '2.4rem' }}>Trust & Safety Pulse</h2>
          <p style={{ color: 'var(--muted)' }}>Live telemetry from ICAP adaptor, Redis queue, and LLM worker.</p>
        </div>
        <button className="cta-button" onClick={() => navigate('/reports')}>Open Reports</button>
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
          <p className="section-title">Operator Focus</p>
          <p style={{ margin: '0 0 0.8rem' }}>
            Manual decisions now run through the domain Allow / Deny list and Pending Sites workflow.
          </p>
          <div style={{ display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
            <button className="cta-button" onClick={() => navigate('/overrides')}>Open Allow / Deny</button>
            <button
              className="cta-button"
              style={{ background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
              onClick={() => navigate('/classifications/pending')}
            >
              Open Pending Sites
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
