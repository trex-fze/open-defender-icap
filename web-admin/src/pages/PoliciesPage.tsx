import { Link, useNavigate } from 'react-router-dom';
import { useState } from 'react';
import { PaginationControls } from '../components/PaginationControls';
import { usePoliciesData } from '../hooks/usePoliciesData';
import { usePolicyMutations } from '../hooks/usePolicyMutations';
import { useAuth } from '../context/AuthContext';

export const PoliciesPage = () => {
  const navigate = useNavigate();
  const { hasRole } = useAuth();
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const [limit, setLimit] = useState(50);
  const [statusFilter, setStatusFilter] = useState('all');
  const [searchText, setSearchText] = useState('');
  const [includeDrafts, setIncludeDrafts] = useState(true);
  const [message, setMessage] = useState<string | undefined>();
  const canEdit = hasRole('policy-editor') || hasRole('policy-admin');
  const canPublish = hasRole('policy-admin');
  const {
    publishPolicy,
    disablePolicy,
    deletePolicy,
    busy,
    error: mutationError,
    canCallApi,
  } = usePolicyMutations();
  const { data: policyRows, meta, loading, error, isMock } = usePoliciesData(cursor, limit, {
    status: statusFilter,
    search: searchText,
    includeDrafts,
  });
  const paginationMeta = meta ?? { has_more: false, next_cursor: undefined, limit };

  const resetPagination = () => {
    setCursor(undefined);
    setCursorStack([]);
  };

  const chipTone = (status: string) => {
    if (status === 'active') return 'green';
    if (status === 'archived') return 'slate';
    return 'amber';
  };

  const onActivate = async (policyId: string, name: string) => {
    setMessage(undefined);
    try {
      await publishPolicy(policyId, `Activated via web-admin for ${name}`);
      setMessage(`Policy ${name} activated.`);
    } catch {
      setMessage(undefined);
    }
  };

  const onDisable = async (policyId: string, name: string, status: string) => {
    if (status === 'archived') {
      setMessage(`Policy ${name} is already disabled.`);
      return;
    }
    if (!window.confirm(`Disable policy \"${name}\"? It will be set to archived status.`)) return;
    setMessage(undefined);
    try {
      await disablePolicy(policyId, `Disabled via web-admin for ${name}`);
      setMessage(`Policy ${name} disabled.`);
    } catch {
      setMessage(undefined);
    }
  };

  const onDelete = async (policyId: string, name: string) => {
    if (
      !window.confirm(
        `Hard delete policy \"${name}\"? This permanently removes metadata, versions, and rules for this policy.`,
      )
    ) {
      return;
    }
    setMessage(undefined);
    try {
      await deletePolicy(policyId);
      setMessage(`Policy ${name} deleted.`);
    } catch {
      setMessage(undefined);
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Policies</p>
          <h2 style={{ margin: 0 }}>Decision Templates</h2>
        </div>
        {canEdit ? (
          <button className="cta-button" onClick={() => navigate('/policies/new')}>
            New Draft
          </button>
        ) : null}
      </div>

      <div className="glass-panel" style={{ display: 'grid', gap: '0.8rem' }}>
        <div style={{ display: 'grid', gridTemplateColumns: 'minmax(220px,1fr) 180px', gap: '0.8rem' }}>
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
              Search
            </span>
            <input
              className="search-input"
              value={searchText}
              onChange={(event) => {
                setSearchText(event.target.value);
                resetPagination();
              }}
              placeholder="Search by policy name, version, or ID"
            />
          </label>
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem', color: 'var(--muted)', fontSize: '0.82rem' }}>
              Status
            </span>
            <select
              className="search-input"
              value={statusFilter}
              onChange={(event) => {
                setStatusFilter(event.target.value);
                resetPagination();
              }}
            >
              <option value="all">All</option>
              <option value="active">Active</option>
              <option value="draft">Draft</option>
              <option value="archived">Archived</option>
            </select>
          </label>
        </div>
        <label style={{ display: 'inline-flex', alignItems: 'center', gap: '0.45rem', fontSize: '0.9rem' }}>
          <input
            type="checkbox"
            checked={includeDrafts}
            onChange={(event) => {
              setIncludeDrafts(event.target.checked);
              resetPagination();
            }}
          />
          Include drafts in list query
        </label>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: 'var(--status-error)' }}>Failed to load live data: {error}</p>
          <p style={{ marginTop: '0.35rem', color: 'var(--muted)' }}>
            Showing cached mock data so you can keep iterating.
          </p>
        </div>
      ) : null}

      {mutationError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: 'var(--status-error)' }}>Policy action failed: {mutationError}</p>
        </div>
      ) : null}

      {message ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: 'var(--status-success)' }}>{message}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: 'var(--citrus)', marginTop: '0.5rem' }}>
          Mock stream (Admin API offline)
        </p>
      ) : null}

      <div className="glass-panel">
        {loading ? (
          <div>
            {Array.from({ length: 3 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.85rem' }}></div>
            ))}
          </div>
        ) : (
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Policies table">
            <table>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Version</th>
                  <th>Status</th>
                  <th>Rules</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {policyRows.map((policy) => (
                  <tr key={policy.id}>
                    <td>{policy.name}</td>
                    <td>{policy.version}</td>
                    <td>
                      <span className={`chip chip--${chipTone(policy.status)}`}>
                        {policy.status}
                      </span>
                    </td>
                    <td>{policy.ruleCount}</td>
                    <td>
                      <div style={{ display: 'flex', gap: '0.45rem', flexWrap: 'wrap' }}>
                        <Link
                          to={`/policies/${policy.id}`}
                          className="nav-link"
                          style={{ padding: '0.25rem 0.5rem' }}
                        >
                          View
                        </Link>
                        {canPublish && policy.status !== 'active' ? (
                          <button
                            className="cta-button"
                            style={{ padding: '0.3rem 0.65rem', fontSize: '0.74rem' }}
                            disabled={busy || isMock || !canCallApi}
                            onClick={() => onActivate(policy.id, policy.name)}
                          >
                            Activate
                          </button>
                        ) : null}
                        {canEdit ? (
                          <button
                            className="cta-button"
                            style={{
                              padding: '0.3rem 0.65rem',
                              fontSize: '0.74rem',
                              background: 'var(--button-secondary-bg)',
                              color: 'var(--button-contrast-text)',
                            }}
                            disabled={busy || isMock || !canCallApi || policy.status === 'active'}
                            onClick={() => onDisable(policy.id, policy.name, policy.status)}
                          >
                            Disable
                          </button>
                        ) : null}
                        {canPublish ? (
                          <button
                            className="cta-button"
                            style={{
                              padding: '0.3rem 0.65rem',
                              fontSize: '0.74rem',
                              background: 'var(--button-danger-bg)',
                              color: 'var(--button-contrast-text)',
                            }}
                            disabled={busy || isMock || !canCallApi || policy.status === 'active'}
                            onClick={() => onDelete(policy.id, policy.name)}
                          >
                            Delete
                          </button>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <PaginationControls
          limit={limit}
          loading={loading}
          hasMore={Boolean(paginationMeta.next_cursor) && paginationMeta.has_more}
          canGoBack={cursorStack.length > 0}
          onLimitChange={(nextLimit) => {
            setLimit(nextLimit);
            setCursor(undefined);
            setCursorStack([]);
          }}
          onPrev={() => {
            if (cursorStack.length === 0) return;
            const stack = [...cursorStack];
            const prev = stack.pop();
            setCursorStack(stack);
            setCursor(prev || undefined);
          }}
          onNext={() => {
            if (!paginationMeta.next_cursor) return;
            setCursorStack((prev) => [...prev, cursor ?? '']);
            setCursor(paginationMeta.next_cursor);
          }}
        />
      </div>
    </div>
  );
};
