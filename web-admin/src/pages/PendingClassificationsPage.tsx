import { PendingClassification, usePendingClassifications } from '../hooks/usePendingClassifications';

export const PendingClassificationsPage = () => {
  const { data, loading, error, isMock, refresh, baseUrl, headers, canCallApi } =
    usePendingClassifications();

  const handleManualUnblock = async (record: PendingClassification) => {
    if (!canCallApi || !baseUrl) {
      alert('Admin API is not reachable. Configure VITE_ADMIN_API_URL and access token.');
      return;
    }
    const action = window.prompt('Enter action (Allow/Block/Warn/Monitor)', 'Allow');
    if (!action) return;
    const reason = window.prompt('Enter analyst note', 'Manual unblock');
    const payload = {
      action,
      primary_category: 'Manual Override',
      subcategory: 'Analyst Override',
      risk_level: 'low',
      confidence: 0.95,
      reason,
    };
    try {
      const resp = await fetch(
        `${baseUrl}/api/v1/classifications/${encodeURIComponent(record.normalizedKey)}/unblock`,
        {
          method: 'POST',
          headers: {
            ...headers,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify(payload),
        },
      );
      if (!resp.ok) {
        const text = await resp.text();
        throw new Error(text || `request failed (${resp.status})`);
      }
      refresh();
    } catch (err) {
      alert(err instanceof Error ? err.message : 'Failed to unblock site');
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Pending Sites</p>
          <h2 style={{ margin: 0 }}>Content-first classification queue</h2>
        </div>
        <button className="cta-button" onClick={refresh} disabled={loading}>
          Refresh
        </button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load pending sites: {error}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: '#fdd744', marginTop: '0.5rem' }}>
          Using mock data (Admin API offline)
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
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Pending classifications table">
            <table>
              <thead>
                <tr>
                  <th>Key</th>
                  <th>Status</th>
                  <th>Base URL</th>
                  <th>Updated</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {data.length === 0 ? (
                  <tr>
                    <td colSpan={5} style={{ textAlign: 'center', color: '#7f8fb2' }}>
                      No pending sites.
                    </td>
                  </tr>
                ) : (
                  data.map((item) => (
                    <tr key={item.normalizedKey}>
                      <td>{item.normalizedKey}</td>
                      <td>{item.status}</td>
                      <td>{item.baseUrl ?? '—'}</td>
                      <td>{item.updatedAt}</td>
                      <td style={{ textAlign: 'right' }}>
                        <button className="ghost-button" onClick={() => handleManualUnblock(item)}>
                          Manual Unblock
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
