import { useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { usePolicyDetail, usePolicyVersions } from '../hooks/usePoliciesData';
import { usePolicyMutations } from '../hooks/usePolicyMutations';
import { useAuth } from '../context/AuthContext';

export const PolicyDetailPage = () => {
  const { policyId } = useParams<{ policyId: string }>();
  const navigate = useNavigate();
  const { hasRole } = useAuth();
  const { data: policy, loading, error, isMock } = usePolicyDetail(policyId);
  const { data: versions, error: versionsError } = usePolicyVersions(policyId);
  const { publishPolicy, updatePolicy, busy, error: mutationError, canCallApi } = usePolicyMutations();
  const [publishMessage, setPublishMessage] = useState<string | undefined>();
  const [saveMessage, setSaveMessage] = useState<string | undefined>();
  const [rulesJson, setRulesJson] = useState('[]');
  const [draftVersion, setDraftVersion] = useState('');
  const canPublish = hasRole('policy-admin');
  const canEdit = hasRole('policy-editor') || hasRole('policy-admin');

  useEffect(() => {
    if (!policy) return;
    setDraftVersion(policy.version);
    setRulesJson(JSON.stringify(policy.rules, null, 2));
  }, [policy?.id, policy?.version, policy?.rules]);

  const handlePublish = async () => {
    if (!policyId || !policy) return;
    try {
      await publishPolicy(policyId, `Published via web-admin for ${policy.name}`);
      setPublishMessage('Policy published successfully. Refresh the list to verify active version.');
      } catch {
      setPublishMessage(undefined);
    }
  };

  const handleSave = async () => {
    if (!policyId || !policy) return;
    try {
      const parsed = JSON.parse(rulesJson);
      if (!Array.isArray(parsed)) {
        setSaveMessage('Rules JSON must be an array of rule objects.');
        return;
      }
      const normalized = parsed.map((rule) => ({
        id: String(rule.id ?? '').trim(),
        description: typeof rule.description === 'string' ? rule.description : '',
        priority: Number(rule.priority ?? 0),
        action: String(rule.action ?? '').trim(),
        conditions:
          rule.conditions && typeof rule.conditions === 'object' && !Array.isArray(rule.conditions)
            ? rule.conditions
            : {},
      }));
      await updatePolicy(policyId, {
        version: draftVersion,
        notes: `Updated via web-admin for ${policy.name}`,
        rules: normalized,
      });
      setSaveMessage('Policy draft saved successfully.');
    } catch {
      setSaveMessage('Save failed. Check JSON syntax and required rule fields.');
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
          {canPublish ? (
            <button
              className="cta-button"
              style={{ background: 'linear-gradient(120deg,#ff9b9b,#fdd744)', color: '#060b17' }}
              onClick={handlePublish}
              disabled={busy || isMock || !canCallApi}
            >
              {busy ? 'Publishing...' : 'Publish Draft'}
            </button>
          ) : null}
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

      {saveMessage ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: '#9ef7eb' }}>{saveMessage}</p>
        </div>
      ) : null}

      {canEdit ? (
        <div className="glass-panel">
          <p className="section-title">Edit Draft</p>
          <div style={{ display: 'grid', gap: '0.75rem' }}>
            <label style={{ display: 'grid', gap: '0.35rem' }}>
              <span style={{ color: 'var(--muted)', fontSize: '0.85rem' }}>Version</span>
              <input
                value={draftVersion}
                onChange={(event) => setDraftVersion(event.target.value)}
                className="input"
              />
            </label>
            <label style={{ display: 'grid', gap: '0.35rem' }}>
              <span style={{ color: 'var(--muted)', fontSize: '0.85rem' }}>Rules JSON</span>
              <textarea
                value={rulesJson}
                onChange={(event) => setRulesJson(event.target.value)}
                rows={16}
                className="input"
                style={{ fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace' }}
              />
            </label>
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <button className="cta-button" onClick={handleSave} disabled={busy || isMock || !canCallApi}>
                {busy ? 'Saving...' : 'Save Draft'}
              </button>
            </div>
          </div>
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

      <div className="glass-panel">
        <p className="section-title">Version History</p>
        {versionsError ? <p style={{ color: '#ff9b9b' }}>Failed to load version history: {versionsError}</p> : null}
        <div className="table-wrapper" role="region" tabIndex={0} aria-label="Policy version history table">
          <table>
            <thead>
              <tr>
                <th>Version</th>
                <th>Status</th>
                <th>Rules</th>
                <th>Created</th>
                <th>Deployed</th>
                <th>Actor</th>
              </tr>
            </thead>
            <tbody>
              {versions.length === 0 ? (
                <tr>
                  <td colSpan={6} style={{ color: 'var(--muted)' }}>
                    No historical versions yet.
                  </td>
                </tr>
              ) : (
                versions.map((item) => (
                  <tr key={item.id}>
                    <td>{item.version}</td>
                    <td>{item.status}</td>
                    <td>{item.ruleCount}</td>
                    <td>{new Date(item.createdAt).toLocaleString()}</td>
                    <td>{item.deployedAt ? new Date(item.deployedAt).toLocaleString() : '—'}</td>
                    <td>{item.createdBy ?? '—'}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
