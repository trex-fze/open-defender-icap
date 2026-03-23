import { Link, useNavigate } from 'react-router-dom';
import { policies } from '../data/mockData';

export const PoliciesPage = () => {
  const navigate = useNavigate();
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

      <div className="glass-panel">
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
              {policies.map((policy) => (
                <tr key={policy.id}>
                  <td>{policy.name}</td>
                  <td>{policy.version}</td>
                  <td>
                    <span className={`chip chip--${policy.status === 'active' ? 'green' : 'amber'}`}>
                      {policy.status}
                    </span>
                  </td>
                  <td>{policy.rules.length}</td>
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
      </div>
    </div>
  );
};
