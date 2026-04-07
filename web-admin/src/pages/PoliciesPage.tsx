import { Link, useNavigate } from 'react-router-dom';
import { useState } from 'react';
import { PaginationControls } from '../components/PaginationControls';
import { usePoliciesData } from '../hooks/usePoliciesData';

export const PoliciesPage = () => {
  const navigate = useNavigate();
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const [limit, setLimit] = useState(50);
  const { data: policyRows, meta, loading, error, isMock } = usePoliciesData(cursor, limit);
  const paginationMeta = meta ?? { has_more: false, next_cursor: undefined, limit };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Policies</p>
          <h2 style={{ margin: 0 }}>Decision Templates</h2>
        </div>
        <button className="cta-button" onClick={() => navigate('/policies/new')}>
          New Draft
        </button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load live data: {error}</p>
          <p style={{ marginTop: '0.35rem', color: 'var(--muted)' }}>
            Showing cached mock data so you can keep iterating.
          </p>
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
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {policyRows.map((policy) => (
                  <tr key={policy.id}>
                    <td>{policy.name}</td>
                    <td>{policy.version}</td>
                    <td>
                      <span className={`chip chip--${policy.status === 'active' ? 'green' : 'amber'}`}>
                        {policy.status}
                      </span>
                    </td>
                    <td>{policy.ruleCount}</td>
                    <td>
                      <Link to={`/policies/${policy.id}`} className="nav-link" style={{ padding: '0.25rem 0.5rem' }}>
                        View
                      </Link>
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
