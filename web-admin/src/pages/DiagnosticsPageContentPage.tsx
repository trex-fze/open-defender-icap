import { FormEvent, useState } from 'react';
import { usePageContentInspector } from '../hooks/usePageContentInspector';

type Attempt = {
  url: string;
  outcome: string;
  reason: string;
};

const parseAttempts = (raw?: string): Attempt[] => {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (item): item is Attempt =>
        item && typeof item.url === 'string' && typeof item.outcome === 'string' && typeof item.reason === 'string',
    );
  } catch {
    return [];
  }
};

const failureHint = (status: string, reason?: string) => {
  if (status === 'unsupported' && reason === 'asset_endpoint') {
    return 'Likely asset/CDN endpoint. Classification now uses apex/www candidates to find real page content.';
  }
  if (status === 'unsupported' && reason === 'no_content_endpoint') {
    return 'Page returned minimal structural content. Try a homepage or product/docs path for richer evidence.';
  }
  if (status === 'unsupported' && reason === 'dns_unresolvable') {
    return 'Candidate hosts did not resolve in DNS during preflight. Inspect source URL/candidate targeting and domain DNS health.';
  }
  if (status === 'blocked') {
    return 'Destination appears protected by anti-bot controls; monitor fallback classification behavior and override if needed.';
  }
  return undefined;
};

export const DiagnosticsPageContentPage = () => {
  const { lookup, record, history, loading, error, canCallApi } = usePageContentInspector();
  const [key, setKey] = useState('domain:example.com');
  const attempts = parseAttempts(record?.attempt_summary);
  const hint = record ? failureHint(record.fetch_status, record.fetch_reason) : undefined;

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
          <h2 style={{ margin: 0 }}>Page Content Inspector (Markdown)</h2>
        </div>
      </div>

      <form className="glass-panel" onSubmit={onLookup}>
        <label>
          <span style={{ display: 'block', marginBottom: '0.35rem' }}>Normalized key</span>
          <input className="search-input" value={key} onChange={(event) => setKey(event.target.value)} />
        </label>
        <p style={{ marginTop: '0.5rem', marginBottom: 0, color: 'var(--muted)' }}>
          This view shows Markdown/plain-text excerpt stored for LLM classification.
        </p>
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
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Content Type:</strong> {record.content_type ?? 'text/markdown'}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Excerpt Format:</strong> {record.excerpt_format ?? 'unknown'}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Source URL:</strong> {record.source_url ?? 'unknown'}
          </p>
          <p style={{ margin: '0.3rem 0' }}>
            <strong>Resolved URL:</strong> {record.resolved_url ?? 'n/a'}
          </p>
          {hint ? (
            <p style={{ margin: '0.6rem 0', color: 'var(--muted)' }}>
              <strong>Hint:</strong> {hint}
            </p>
          ) : null}

          {attempts.length > 0 ? (
            <div style={{ marginTop: '0.9rem' }}>
              <p className="section-title">Fetch Attempts</p>
              <div className="table-wrapper" role="region" tabIndex={0} aria-label="Fetch attempts table">
                <table>
                  <thead>
                    <tr>
                      <th>URL</th>
                      <th>Outcome</th>
                      <th>Reason</th>
                    </tr>
                  </thead>
                  <tbody>
                    {attempts.map((attempt, index) => (
                      <tr key={`${attempt.url}-${index}`}>
                        <td>{attempt.url}</td>
                        <td>{attempt.outcome}</td>
                        <td>{attempt.reason}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          ) : null}

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
                  <th>Reason</th>
                  <th>Fetched</th>
                  <th>Expires</th>
                  <th>Chars</th>
                  <th>Resolved URL</th>
                </tr>
              </thead>
              <tbody>
                {history.map((row) => (
                  <tr key={row.fetch_version}>
                    <td>{row.fetch_version}</td>
                    <td>{row.fetch_status}</td>
                    <td>{row.fetch_reason ?? '-'}</td>
                    <td>{new Date(row.fetched_at).toLocaleString()}</td>
                    <td>{new Date(row.expires_at).toLocaleString()}</td>
                    <td>{row.char_count ?? 0}</td>
                    <td>{row.resolved_url ?? '-'}</td>
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
