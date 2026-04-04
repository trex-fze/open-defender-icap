import { FormEvent, useMemo, useState } from 'react';
import { PaginationControls } from '../components/PaginationControls';
import { useOverrideActions } from '../hooks/useOverrideActions';
import { useOverridesData } from '../hooks/useOverridesData';

const ACTION_OPTIONS = ['allow', 'block'];
const STATUS_OPTIONS = ['active', 'inactive', 'expired', 'revoked'];

export const OverridesPage = () => {
  const [limit, setLimit] = useState(50);
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const { data, meta, loading, error, isMock, refresh, canCallApi } = useOverridesData(cursor, limit);
  const paginationMeta = meta ?? { has_more: false, next_cursor: undefined };
  const { createOverride, updateOverride, deleteOverride, busy, error: actionError } = useOverrideActions();

  const [editingId, setEditingId] = useState<string | undefined>();
  const scopeType = 'domain';
  const [scopeValue, setScopeValue] = useState('');
  const [action, setAction] = useState('allow');
  const [status, setStatus] = useState('active');
  const [reason, setReason] = useState('');
  const [expiresAt, setExpiresAt] = useState('');
  const [message, setMessage] = useState<string | undefined>();

  const editing = useMemo(() => data.find((row) => row.id === editingId), [data, editingId]);

  const resetForm = () => {
    setEditingId(undefined);
    setScopeValue('');
    setAction('allow');
    setStatus('active');
    setReason('');
    setExpiresAt('');
  };

  const loadForEdit = (id: string) => {
    const row = data.find((item) => item.id === id);
    if (!row) return;
    setEditingId(row.id);
    setScopeValue(row.scopeValue);
    setAction(row.action);
    setStatus(row.status);
    setReason(row.reason ?? '');
    setExpiresAt(row.expiresAt ?? '');
    setMessage(undefined);
  };

  const submitOverride = async (event: FormEvent) => {
    event.preventDefault();
    setMessage(undefined);

    if (!scopeValue.trim()) {
      setMessage('Scope value is required');
      return;
    }

    try {
      const payload = {
        scopeType,
        scopeValue,
        action,
        status,
        reason,
        expiresAt,
      };

      if (editingId) {
        await updateOverride(editingId, payload);
        setMessage('Override updated successfully');
      } else {
        await createOverride(payload);
        setMessage('Override created successfully');
      }

      resetForm();
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  const onDelete = async (id: string) => {
    setMessage(undefined);
    try {
      await deleteOverride(id);
      if (editingId === id) {
        resetForm();
      }
      setMessage(`Override ${id} deleted`);
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Allow / Deny List</p>
          <h2 style={{ margin: 0 }}>Domain-level manual decisions</h2>
        </div>
        <button className="cta-button" onClick={refresh} disabled={loading}>
          Refresh
        </button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load overrides: {error}</p>
        </div>
      ) : null}

      {actionError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Override change failed: {actionError}</p>
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

      <form className="glass-panel" onSubmit={submitOverride}>
        <p className="section-title">{editing ? `Edit Entry ${editing.id}` : 'Create Entry'}</p>
        <div className="layout-grid">
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>Domain</span>
            <input
              className="search-input"
              value={scopeValue}
              onChange={(event) => setScopeValue(event.target.value)}
              placeholder="example.com"
            />
          </label>
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>Decision</span>
            <select className="search-input" value={action} onChange={(event) => setAction(event.target.value)}>
              {ACTION_OPTIONS.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>Status</span>
            <select className="search-input" value={status} onChange={(event) => setStatus(event.target.value)}>
              {STATUS_OPTIONS.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>Expires at (ISO-8601, optional)</span>
            <input
              className="search-input"
              value={expiresAt}
              onChange={(event) => setExpiresAt(event.target.value)}
              placeholder="2026-12-31T23:59:59Z"
            />
          </label>
          <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Reason (optional)</span>
              <input
                className="search-input"
                value={reason}
                onChange={(event) => setReason(event.target.value)}
                placeholder="Temporary domain exception"
              />
            </label>
          </div>

        <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
          <button
            className="cta-button"
            type="submit"
            disabled={busy || !canCallApi || isMock || !scopeValue.trim()}
          >
              {busy ? 'Saving...' : editing ? 'Update Entry' : 'Create Entry'}
          </button>
          <button
            className="cta-button"
            type="button"
            style={{ background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
            onClick={resetForm}
          >
            Clear
          </button>
        </div>
      </form>

      <div className="glass-panel">
        <PaginationControls
          limit={limit}
          loading={loading}
          hasMore={Boolean(paginationMeta.next_cursor) && paginationMeta.has_more}
          canGoBack={cursorStack.length > 0}
          onPrev={() => {
            setCursorStack((prev) => {
              if (prev.length === 0) return prev;
              const next = [...prev];
              const previousCursor = next.pop();
              setCursor(previousCursor || undefined);
              return next;
            });
          }}
          onNext={() => {
            if (!paginationMeta.next_cursor) return;
            setCursorStack((prev) => [...prev, cursor ?? '']);
            setCursor(paginationMeta.next_cursor);
          }}
          onLimitChange={(nextLimit) => {
            setLimit(nextLimit);
            setCursor(undefined);
            setCursorStack([]);
          }}
        />
        {loading ? (
          <div>
            {Array.from({ length: 4 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : (
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Overrides table">
            <table>
              <thead>
                <tr>
                  <th>Scope</th>
                  <th>Action</th>
                  <th>Expires</th>
                  <th>Status</th>
                  <th>Operations</th>
                </tr>
              </thead>
              <tbody>
                {data.map((item) => (
                  <tr key={item.id}>
                    <td>{item.scope}</td>
                    <td>{item.action}</td>
                    <td>{item.expires}</td>
                    <td>
                      <span className={`chip chip--${item.status === 'active' ? 'green' : 'amber'}`}>
                        {item.status}
                      </span>
                    </td>
                    <td>
                      <div style={{ display: 'flex', gap: '0.45rem', flexWrap: 'wrap' }}>
                        <button
                          className="cta-button"
                          style={{ padding: '0.4rem 0.8rem', fontSize: '0.75rem' }}
                          onClick={() => loadForEdit(item.id)}
                          disabled={busy}
                        >
                          Edit
                        </button>
                        <button
                          className="cta-button"
                          style={{
                            padding: '0.4rem 0.8rem',
                            fontSize: '0.75rem',
                            background: 'linear-gradient(120deg,#ff9b9b,#fdd744)',
                            color: '#060b17',
                          }}
                          onClick={() => onDelete(item.id)}
                          disabled={busy || !canCallApi || isMock}
                        >
                          Delete
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
