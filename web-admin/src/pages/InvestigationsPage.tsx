import { useMemo, useState } from 'react';
import { useReviewQueueData } from '../hooks/useReviewQueueData';

export const InvestigationsPage = () => {
  const [query, setQuery] = useState('');
  const { data, loading, error, isMock } = useReviewQueueData();

  const investigations = useMemo(
    () =>
      data.map((item) => ({
        key: item.key,
        verdict: item.status,
        risk: item.risk,
        lastSeen: item.sla,
        tags: item.assignedTo ? [item.assignedTo] : [],
      })),
    [data],
  );

  const filtered = useMemo(() => {
    if (!query.trim()) return investigations;
    const term = query.trim().toLowerCase();
    return investigations.filter((item) => item.key.toLowerCase().includes(term));
  }, [investigations, query]);

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Investigations</p>
          <h2 style={{ margin: 0 }}>Classification History & Cache</h2>
        </div>
        <button className="cta-button">Open Timeline</button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Unable to reach Admin API: {error}</p>
          <p style={{ color: 'var(--muted)' }}>Showing mock data while we retry the live feed.</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: '#fdd744', marginTop: '0.5rem' }}>
          Mock stream (Admin API offline)
        </p>
      ) : null}

      <input
        className="search-input"
        placeholder="Search normalized key, domain, reviewer..."
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />

      <div className="glass-panel" style={{ marginTop: '1.5rem' }}>
        {loading ? (
          <div>
            {Array.from({ length: 4 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : (
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Investigations table">
            <table>
              <thead>
                <tr>
                  <th>Key</th>
                  <th>Verdict</th>
                  <th>Risk</th>
                  <th>Last Seen</th>
                  <th>Tags</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((item) => (
                  <tr key={item.key}>
                    <td>{item.key}</td>
                    <td>
                      <span className={`chip chip--${item.verdict === 'Block' ? 'red' : 'amber'}`}>
                        {item.verdict}
                      </span>
                    </td>
                    <td>{item.risk}</td>
                    <td>{item.lastSeen}</td>
                    <td>
                      {item.tags.length === 0 ? (
                        <span className="chip" style={{ color: '#94a6cc' }}>
                          Unassigned
                        </span>
                      ) : (
                        item.tags.map((tag) => (
                          <span key={tag} className="chip" style={{ marginRight: '0.35rem' }}>
                            {tag}
                          </span>
                        ))
                      )}
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
