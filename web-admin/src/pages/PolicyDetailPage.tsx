import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { usePolicyDetail } from '../hooks/usePoliciesData';
import { usePolicyMutations } from '../hooks/usePolicyMutations';

export const PolicyDetailPage = () => {
  const { policyId } = useParams<{ policyId: string }>();
  const navigate = useNavigate();
  const { data: policy, loading, error, isMock } = usePolicyDetail(policyId);
  const { publishPolicy, busy, error: mutationError, canCallApi } = usePolicyMutations();
  const [publishMessage, setPublishMessage] = useState<string | undefined>();

  const handlePublish = async () => {
    if (!policyId || !policy) return;
    try {
      await publishPolicy(policyId, `Published via web-admin for ${policy.name}`);
      setPublishMessage('Policy published successfully. Refresh the list to verify active version.');
    } catch {
      setPublishMessage(undefined);
    }
  };

  if (loading) {
    return (
      <div className="glass-panel">
        <p className="section-title">Loading policy</p>
        {Array.from({ length: 4 }).map((_, idx) => (
          <div key={idx} className="skeleton" style={{ marginBottom: '0.65rem' }}></div>
        ))}
      </div>
    );
  }

  if (!policy) {
    return (
      <div className="glass-panel">
        <p style={{ color: '#ff9b9b', marginTop: 0 }}>Policy not found.</p>
        {error ? <p style={{ color: 'var(--muted)' }}>{error}</p> : null}
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
          <p style={{ color: 'var(--muted)' }}>Version {policy.version}</p>
        </div>
        <div style={{ display: 'flex', gap: '0.75rem', flexWrap: 'wrap' }}>
          {isMock ? (
            <span className="chip chip--amber">Mock</span>
          ) : (
            <span className="chip chip--green">Live</span>
          )}
          <button className="cta-button" onClick={() => navigate('/policies')}>
            Back
          </button>
          <button
            className="cta-button"
            style={{ background: 'linear-gradient(120deg,#ff9b9b,#fdd744)', color: '#060b17' }}
            onClick={handlePublish}
            disabled={busy || isMock || !canCallApi}
          >
            {busy ? 'Publishing...' : 'Publish Draft'}
          </button>
        </div>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Unable to reach Admin API: {error}</p>
        </div>
      ) : null}

      {mutationError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Publish failed: {mutationError}</p>
        </div>
      ) : null}

      {publishMessage ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: '#9ef7eb' }}>{publishMessage}</p>
        </div>
      ) : null}

      <div className="glass-panel">
        <p className="section-title">Rules</p>
        <div className="table-wrapper" role="region" tabIndex={0} aria-label="Policy rules table">
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
                    <span className={`chip chip--${rule.action === 'Block' ? 'red' : 'amber'}`}>
                      {rule.action}
                    </span>
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
