import { FormEvent, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { usePolicyMutations } from '../hooks/usePolicyMutations';

export const PolicyCreatePage = () => {
  const navigate = useNavigate();
  const { createDraft, busy, error, canCallApi } = usePolicyMutations();
  const [name, setName] = useState('');
  const [version, setVersion] = useState('');
  const [notes, setNotes] = useState('');
  const [success, setSuccess] = useState<string | undefined>();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setSuccess(undefined);
    const trimmedName = name.trim();
    if (!trimmedName) {
      return;
    }

    try {
      const policyId = await createDraft({
        name: trimmedName,
        version,
        notes,
      });
      setSuccess('Draft created. Redirecting to policy detail...');
      setTimeout(() => navigate(`/policies/${policyId}`), 500);
    } catch {
      // hook exposes error
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Policies</p>
          <h2 style={{ margin: 0 }}>Create New Draft</h2>
          <p style={{ color: 'var(--muted)', marginBottom: 0 }}>
            A starter monitor rule will be added automatically. Edit rules in the policy detail page.
          </p>
        </div>
      </div>

      {!canCallApi ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 225, 127, 0.45)' }}>
          <p style={{ margin: 0, color: '#ffe17f' }}>
            Admin API is not configured for live mutations. Set `VITE_ADMIN_API_URL` and sign in with a valid token.
          </p>
        </div>
      ) : null}

      <form onSubmit={handleSubmit} className="glass-panel" style={{ maxWidth: 760 }}>
        <div style={{ display: 'grid', gap: '0.9rem' }}>
          <label>
            <span className="section-title" style={{ marginBottom: '0.35rem', display: 'block' }}>
              Name
            </span>
            <input
              className="search-input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Example: Corporate Safe Browsing Draft"
              required
            />
          </label>

          <label>
            <span className="section-title" style={{ marginBottom: '0.35rem', display: 'block' }}>
              Version (optional)
            </span>
            <input
              className="search-input"
              value={version}
              onChange={(event) => setVersion(event.target.value)}
              placeholder="draft-20260325"
            />
          </label>

          <label>
            <span className="section-title" style={{ marginBottom: '0.35rem', display: 'block' }}>
              Notes (optional)
            </span>
            <textarea
              className="search-input"
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
              placeholder="What changed and why"
              rows={4}
            />
          </label>
        </div>

        {error ? (
          <p style={{ color: '#ff9b9b', marginBottom: 0 }}>Failed to create draft: {error}</p>
        ) : null}
        {success ? <p style={{ color: '#9ef7eb', marginBottom: 0 }}>{success}</p> : null}

        <div style={{ marginTop: '1.2rem', display: 'flex', gap: '0.7rem', flexWrap: 'wrap' }}>
          <button className="cta-button" type="submit" disabled={busy || !canCallApi || !name.trim()}>
            {busy ? 'Creating...' : 'Create Draft'}
          </button>
          <button
            className="cta-button"
            type="button"
            style={{ background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
            onClick={() => navigate('/policies')}
          >
            Cancel
          </button>
        </div>
      </form>
    </div>
  );
};
