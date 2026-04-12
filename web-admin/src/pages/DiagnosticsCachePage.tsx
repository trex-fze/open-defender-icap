import { FormEvent, useState } from 'react';
import { useCacheDiagnostics } from '../hooks/useCacheDiagnostics';

export const DiagnosticsCachePage = () => {
  const { lookup, evict, entry, loading, error, message, canCallApi } = useCacheDiagnostics();
  const [key, setKey] = useState('domain:example.com');

  const onLookup = async (event: FormEvent) => {
    event.preventDefault();
    if (!key.trim()) return;
    await lookup(key);
  };

  const onEvict = async () => {
    if (!key.trim()) return;
    await evict(key);
  };

  return (
    <div className="diagnostics-tool">
      <form className="diagnostics-section" onSubmit={onLookup}>
        <p className="section-title">Cache Entry Inspector</p>
        <label>
          <span style={{ display: 'block', marginBottom: '0.35rem' }}>Cache key</span>
          <input className="search-input" value={key} onChange={(event) => setKey(event.target.value)} />
        </label>
        <p style={{ marginTop: '0.5rem', marginBottom: 0, color: 'var(--muted)' }}>
          Use normalized keys such as <code>subdomain:www.instagram.com</code> or <code>domain:example.com</code>.
        </p>
        <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
          <button className="cta-button" disabled={loading || !canCallApi || !key.trim()}>
            {loading ? 'Loading...' : 'Lookup'}
          </button>
          <button
            type="button"
            className="cta-button btn-danger"
            onClick={onEvict}
            disabled={loading || !canCallApi || !key.trim()}
          >
            Evict
          </button>
        </div>
      </form>

      {error ? (
        <div className="diagnostics-section glass-panel--error">
          <p style={{ margin: 0, color: 'var(--status-error)' }}>{error}</p>
        </div>
      ) : null}

      {message ? (
        <div className="diagnostics-section glass-panel--success">
          <p style={{ margin: 0, color: 'var(--status-success)' }}>{message}</p>
        </div>
      ) : null}

      {entry ? (
        <div className="diagnostics-section">
          <p className="section-title">Result</p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Key:</strong> {entry.cache_key}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Source:</strong> {entry.source ?? 'unknown'}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Expires:</strong> {new Date(entry.expires_at).toLocaleString()}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Created:</strong> {new Date(entry.created_at).toLocaleString()}
          </p>
          <pre className="code-block-panel">
            {JSON.stringify(entry.value, null, 2)}
          </pre>
        </div>
      ) : null}
    </div>
  );
};
