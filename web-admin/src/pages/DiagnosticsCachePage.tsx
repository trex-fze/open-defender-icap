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
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Diagnostics</p>
          <h2 style={{ margin: 0 }}>Cache Entry Inspector</h2>
        </div>
      </div>

      <form className="glass-panel" onSubmit={onLookup}>
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
            className="cta-button"
            style={{ background: 'var(--button-danger-bg)', color: 'var(--button-contrast-text)' }}
            onClick={onEvict}
            disabled={loading || !canCallApi || !key.trim()}
          >
            Evict
          </button>
        </div>
      </form>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: 'var(--status-error)' }}>{error}</p>
        </div>
      ) : null}

      {message ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: 'var(--status-success)' }}>{message}</p>
        </div>
      ) : null}

      {entry ? (
        <div className="glass-panel">
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
          <pre
            style={{
              marginTop: '0.9rem',
              background: 'rgba(4, 13, 26, 0.6)',
              border: '1px solid rgba(255,255,255,0.1)',
              borderRadius: '0.8rem',
              padding: '0.8rem',
              overflowX: 'auto',
            }}
          >
            {JSON.stringify(entry.value, null, 2)}
          </pre>
        </div>
      ) : null}
    </div>
  );
};
