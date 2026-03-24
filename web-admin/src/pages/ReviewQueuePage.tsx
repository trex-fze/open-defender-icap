import { useState } from 'react';
import { useReviewQueueActions } from '../hooks/useReviewQueueActions';
import { useReviewQueueData } from '../hooks/useReviewQueueData';

export const ReviewQueuePage = () => {
  const { data, loading, error, isMock, refresh, canCallApi } = useReviewQueueData();
  const { resolveReview, resolvingId, error: resolveError } = useReviewQueueActions();
  const [message, setMessage] = useState<string | undefined>();

  const handleResolve = async (id: string, status: 'approved' | 'rejected', decisionAction: string) => {
    setMessage(undefined);
    try {
      await resolveReview(id, {
        status,
        decision_action: decisionAction,
        decision_notes: `Resolved via web-admin as ${status}`,
      });
      setMessage(`Review ${id} resolved as ${status}`);
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Review Queue</p>
          <h2 style={{ margin: 0 }}>Human-in-the-loop decisions</h2>
        </div>
        <button className="cta-button" onClick={refresh} disabled={loading}>
          Refresh
        </button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load review queue: {error}</p>
        </div>
      ) : null}

      {resolveError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to resolve review item: {resolveError}</p>
        </div>
      ) : null}

      {message ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: '#9ef7eb' }}>{message}</p>
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
                  <th>Actions</th>
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
                    <td>
                      <div style={{ display: 'flex', gap: '0.45rem', flexWrap: 'wrap' }}>
                        <button
                          className="cta-button"
                          style={{ padding: '0.45rem 0.9rem', fontSize: '0.75rem' }}
                          disabled={isMock || !canCallApi || resolvingId === item.id}
                          onClick={() => handleResolve(item.id, 'approved', 'allow')}
                        >
                          {resolvingId === item.id ? 'Saving...' : 'Approve'}
                        </button>
                        <button
                          className="cta-button"
                          style={{
                            padding: '0.45rem 0.9rem',
                            fontSize: '0.75rem',
                            background: 'linear-gradient(120deg,#ff9b9b,#fdd744)',
                            color: '#060b17',
                          }}
                          disabled={isMock || !canCallApi || resolvingId === item.id}
                          onClick={() => handleResolve(item.id, 'rejected', 'block')}
                        >
                          {resolvingId === item.id ? 'Saving...' : 'Reject'}
                        </button>
                      </div>
                    </td>
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
