import { Link, useNavigate } from 'react-router-dom';
import { usePoliciesData } from '../hooks/usePoliciesData';

export const PoliciesPage = () => {
  const navigate = useNavigate();
  const { data: policyRows, loading, error, isMock } = usePoliciesData();

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
          <p style={{ marginTop: '0.35rem', color: '#9fb2d0' }}>
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
          <div className="table-wrapper">
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
      </div>
    </div>
  );
};
