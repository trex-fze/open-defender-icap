import { useReviewQueueData } from '../hooks/useReviewQueueData';

export const ReviewQueuePage = () => {
  const { data, loading, error, isMock } = useReviewQueueData();
  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Review Queue</p>
          <h2 style={{ margin: 0 }}>Human-in-the-loop decisions</h2>
        </div>
        <button className="cta-button">Bulk Resolve</button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load review queue: {error}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: '#fdd744', marginTop: '0.5rem' }}>
          Mock stream (Admin API offline)
        </p>
      ) : null}

      <div className="glass-panel">
        {loading ? (
          <div>
            {Array.from({ length: 5 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : (
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Review queue table">
            <table>
              <thead>
                <tr>
                  <th>Key</th>
                  <th>Status</th>
                  <th>Risk</th>
                  <th>SLA</th>
                </tr>
              </thead>
              <tbody>
                {data.map((item) => (
                  <tr key={item.id}>
                    <td>{item.key}</td>
                    <td>{item.status}</td>
                    <td>
                      <span className={`chip chip--${item.risk === 'critical' ? 'red' : 'amber'}`}>
                        {item.risk}
                      </span>
                    </td>
                    <td>{item.sla}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
