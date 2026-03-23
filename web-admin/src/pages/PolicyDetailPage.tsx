import { useMemo } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { policies } from '../data/mockData';

export const PolicyDetailPage = () => {
  const { policyId } = useParams<{ policyId: string }>();
  const navigate = useNavigate();
  const policy = useMemo(() => policies.find((p) => p.id === policyId), [policyId]);

  if (!policy) {
    return (
      <div className="glass-panel">
        <p>Policy not found.</p>
        <button className="cta-button" onClick={() => navigate('/policies')}>
          Back to policies
        </button>
      </div>
    );
  }

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Policy Detail</p>
          <h2 style={{ margin: 0 }}>{policy.name}</h2>
          <p style={{ color: '#9fb2d0' }}>Version {policy.version}</p>
        </div>
        <div style={{ display: 'flex', gap: '0.75rem' }}>
          <button className="cta-button" onClick={() => navigate('/policies')}>
            Back
          </button>
          <button className="cta-button" style={{ background: 'linear-gradient(120deg,#ff9b9b,#fdd744)', color: '#060b17' }}>
            Publish Draft
          </button>
        </div>
      </div>

      <div className="glass-panel">
        <p className="section-title">Rules</p>
        <div className="table-wrapper">
          <table>
            <thead>
              <tr>
                <th>Priority</th>
                <th>Action</th>
                <th>Description</th>
                <th>Conditions</th>
              </tr>
            </thead>
            <tbody>
              {policy.rules.map((rule) => (
                <tr key={rule.id}>
                  <td>{rule.priority}</td>
                  <td>
                    <span className={`chip chip--${rule.action === 'Block' ? 'red' : 'amber'}`}>{rule.action}</span>
                  </td>
                  <td>{rule.description ?? '—'}</td>
                  <td>
                    <code style={{ fontSize: '0.85rem' }}>{JSON.stringify(rule.conditions)}</code>
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
