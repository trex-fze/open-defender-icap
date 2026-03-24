import { FormEvent, useState } from 'react';
import { usePageContentInspector } from '../hooks/usePageContentInspector';

export const DiagnosticsPageContentPage = () => {
  const { lookup, record, history, loading, error, canCallApi } = usePageContentInspector();
  const [key, setKey] = useState('domain:example.com');

  const onLookup = async (event: FormEvent) => {
    event.preventDefault();
    if (!key.trim()) return;
    await lookup(key);
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Diagnostics</p>
          <h2 style={{ margin: 0 }}>Page Content Inspector</h2>
        </div>
      </div>

      <form className="glass-panel" onSubmit={onLookup}>
        <label>
          <span style={{ display: 'block', marginBottom: '0.35rem' }}>Normalized key</span>
          <input className="search-input" value={key} onChange={(event) => setKey(event.target.value)} />
        </label>
        <div style={{ marginTop: '1rem' }}>
          <button className="cta-button" disabled={!canCallApi || !key.trim() || loading}>
            {loading ? 'Loading...' : 'Lookup'}
          </button>
        </div>
      </form>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>{error}</p>
        </div>
      ) : null}

      {record ? (
        <div className="glass-panel">
          <p className="section-title">Latest Content</p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Version:</strong> {record.fetch_version}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Status:</strong> {record.fetch_status}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Fetched:</strong> {new Date(record.fetched_at).toLocaleString()}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Expires:</strong> {new Date(record.expires_at).toLocaleString()}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Chars:</strong> {record.char_count ?? 0}
          </p>

          <pre
            style={{
              marginTop: '0.9rem',
              background: 'rgba(4, 13, 26, 0.6)',
              border: '1px solid rgba(255,255,255,0.1)',
              borderRadius: '0.8rem',
              padding: '0.8rem',
              whiteSpace: 'pre-wrap',
            }}
          >
            {record.excerpt ?? 'No excerpt available'}
          </pre>
        </div>
      ) : null}

      {history.length > 0 ? (
        <div className="glass-panel">
          <p className="section-title">Version History</p>
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Page content history table">
            <table>
              <thead>
                <tr>
                  <th>Version</th>
                  <th>Status</th>
                  <th>Fetched</th>
                  <th>Expires</th>
                  <th>Chars</th>
                </tr>
              </thead>
              <tbody>
                {history.map((row) => (
                  <tr key={row.fetch_version}>
                    <td>{row.fetch_version}</td>
                    <td>{row.fetch_status}</td>
                    <td>{new Date(row.fetched_at).toLocaleString()}</td>
                    <td>{new Date(row.expires_at).toLocaleString()}</td>
                    <td>{row.char_count ?? 0}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}
    </div>
  );
};
