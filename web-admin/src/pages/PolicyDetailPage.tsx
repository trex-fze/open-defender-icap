import { useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { usePolicyDetail, usePolicyVersions } from '../hooks/usePoliciesData';
import { usePolicyMutations } from '../hooks/usePolicyMutations';
import { useAuth } from '../context/AuthContext';

type EditableRule = {
  id: string;
  description: string;
  priority: number;
  action: string;
  conditions: Record<string, unknown>;
};

const toEditableRules = (input: unknown): EditableRule[] => {
  if (!Array.isArray(input)) return [];
  return input.map((rule) => {
    const candidate = rule as Record<string, unknown>;
    return {
      id: String(candidate.id ?? '').trim(),
      description: typeof candidate.description === 'string' ? candidate.description : '',
      priority: Number(candidate.priority ?? 0),
      action: String(candidate.action ?? '').trim(),
      conditions:
        candidate.conditions && typeof candidate.conditions === 'object' && !Array.isArray(candidate.conditions)
          ? (candidate.conditions as Record<string, unknown>)
          : {},
    };
  });
};

const statusTone = (status: string) => {
  if (status === 'active') return 'green';
  if (status === 'archived') return 'slate';
  return 'amber';
};

export const PolicyDetailPage = () => {
  const { policyId } = useParams<{ policyId: string }>();
  const navigate = useNavigate();
  const { hasRole } = useAuth();
  const { data: policy, loading, error, isMock } = usePolicyDetail(policyId);
  const { data: versions, error: versionsError } = usePolicyVersions(policyId);
  const {
    publishPolicy,
    updatePolicy,
    validatePolicy,
    disablePolicy,
    deletePolicy,
    busy,
    error: mutationError,
    canCallApi,
  } = usePolicyMutations();
  const [message, setMessage] = useState<string | undefined>();
  const [rulesJson, setRulesJson] = useState('[]');
  const [draftVersion, setDraftVersion] = useState('');
  const [publishVersion, setPublishVersion] = useState('');
  const canPublish = hasRole('policy-admin');
  const canEdit = hasRole('policy-editor') || hasRole('policy-admin');

  useEffect(() => {
    if (!policy) return;
    setDraftVersion(policy.version);
    setPublishVersion(policy.version);
    setRulesJson(JSON.stringify(policy.rules, null, 2));
  }, [policy?.id, policy?.version, policy?.rules]);

  const parseRules = (): EditableRule[] => {
    const parsed = JSON.parse(rulesJson);
    const normalized = toEditableRules(parsed);
    if (normalized.length === 0) {
      throw new Error('At least one policy rule is required.');
    }
    return normalized;
  };

  const handleValidate = async () => {
    if (!policy || !policyId) return;
    setMessage(undefined);
    try {
      const normalized = parseRules();
      const response = await validatePolicy({
        name: policy.name,
        version: draftVersion,
        notes: `Validated via web-admin for ${policy.name}`,
        rules: normalized,
      });
      if (response.valid) {
        setMessage('Validation passed. Policy rules are valid.');
      } else {
        setMessage(`Validation failed: ${response.errors.join('; ')}`);
      }
    } catch (err) {
      if (err instanceof Error) {
        setMessage(`Validation failed: ${err.message}`);
        return;
      }
      setMessage('Validation failed due to an unknown error.');
    }
  };

  const handleSave = async () => {
    if (!policyId || !policy) return;
    setMessage(undefined);
    try {
      const normalized = parseRules();
      await updatePolicy(policyId, {
        version: draftVersion,
        notes: `Updated via web-admin for ${policy.name}`,
        rules: normalized,
      });
      setMessage('Policy draft saved successfully.');
    } catch (err) {
      if (err instanceof Error) {
        setMessage(`Save failed: ${err.message}`);
        return;
      }
      setMessage('Save failed. Check JSON syntax and required rule fields.');
    }
  };

  const handleActivate = async () => {
    if (!policyId || !policy) return;
    setMessage(undefined);
    try {
      await publishPolicy(
        policyId,
        `Activated via web-admin for ${policy.name}`,
        publishVersion.trim() || undefined,
      );
      setMessage('Policy activated successfully.');
    } catch {
      setMessage(undefined);
    }
  };

  const handleDisable = async () => {
    if (!policyId || !policy) return;
    if (policy.status === 'active') {
      setMessage('Active policy cannot be disabled directly. Activate another policy first.');
      return;
    }
    if (!window.confirm(`Disable policy \"${policy.name}\" by archiving it?`)) return;
    setMessage(undefined);
    try {
      await disablePolicy(policyId, `Disabled via web-admin for ${policy.name}`);
      setMessage('Policy disabled (archived).');
    } catch {
      setMessage(undefined);
    }
  };

  const handleDelete = async () => {
    if (!policyId || !policy) return;
    if (policy.status === 'active') {
      setMessage('Active policy cannot be hard deleted. Activate another policy first.');
      return;
    }
    if (
      !window.confirm(
        `Hard delete policy \"${policy.name}\"? This permanently removes all versions and rules for this policy.`,
      )
    ) {
      return;
    }
    setMessage(undefined);
    try {
      await deletePolicy(policyId);
      navigate('/policies');
    } catch {
      setMessage(undefined);
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
        <p style={{ color: 'var(--status-error)', marginTop: 0 }}>Policy not found.</p>
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
          <p style={{ color: 'var(--muted)', marginBottom: 0 }}>
            Current Version {policy.version} ·{' '}
            <span className={`chip chip--${statusTone(policy.status)}`}>{policy.status}</span>
          </p>
        </div>
        <div style={{ display: 'flex', gap: '0.75rem', flexWrap: 'wrap' }}>
          {isMock ? <span className="chip chip--amber">Mock</span> : <span className="chip chip--green">Live</span>}
          <button className="cta-button" onClick={() => navigate('/policies')}>
            Back
          </button>
          {canPublish ? (
            <button
              className="cta-button btn-danger"
              onClick={handleActivate}
              disabled={busy || isMock || !canCallApi}
            >
              {busy ? 'Activating...' : 'Activate'}
            </button>
          ) : null}
          {canEdit ? (
            <button
              className="cta-button btn-secondary"
              onClick={handleDisable}
              disabled={busy || isMock || !canCallApi || policy.status === 'active'}
            >
              Disable
            </button>
          ) : null}
          {canPublish ? (
            <button
              className="cta-button btn-danger-strong"
              onClick={handleDelete}
              disabled={busy || isMock || !canCallApi || policy.status === 'active'}
            >
              Hard Delete
            </button>
          ) : null}
        </div>
      </div>

      {error ? (
        <div className="glass-panel glass-panel--error">
          <p style={{ margin: 0, color: 'var(--status-error)' }}>Unable to reach Admin API: {error}</p>
        </div>
      ) : null}

      {mutationError ? (
        <div className="glass-panel glass-panel--error">
          <p style={{ margin: 0, color: 'var(--status-error)' }}>Policy action failed: {mutationError}</p>
        </div>
      ) : null}

      {message ? (
        <div className="glass-panel glass-panel--success">
          <p style={{ margin: 0, color: 'var(--status-success)' }}>{message}</p>
        </div>
      ) : null}

      {canPublish ? (
        <div className="glass-panel">
          <p className="section-title">Activation version</p>
          <label style={{ display: 'grid', gap: '0.35rem', maxWidth: 360 }}>
            <span style={{ color: 'var(--muted)', fontSize: '0.85rem' }}>Version label for activation</span>
            <input
              value={publishVersion}
              onChange={(event) => setPublishVersion(event.target.value)}
              className="input"
              placeholder="release-202604091900"
            />
          </label>
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
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.65rem', flexWrap: 'wrap' }}>
              <button className="cta-button" onClick={handleValidate} disabled={busy || isMock || !canCallApi}>
                {busy ? 'Validating...' : 'Validate'}
              </button>
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

      <div className="glass-panel">
        <p className="section-title">Version History</p>
        {versionsError ? <p style={{ color: 'var(--status-error)' }}>Failed to load version history: {versionsError}</p> : null}
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
