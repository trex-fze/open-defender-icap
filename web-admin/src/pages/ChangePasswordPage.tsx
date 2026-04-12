import { FormEvent, useState } from 'react';
import { Navigate, useNavigate } from 'react-router-dom';
import { adminPostJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from '../hooks/useAdminApi';
import { useAuth } from '../context/AuthContext';

export const ChangePasswordPage = () => {
  const api = useAdminApi();
  const { user, login } = useAuth();
  const navigate = useNavigate();
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();

  if (!user?.mustChangePassword) {
    return <Navigate to="/dashboard" replace />;
  }

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setError(undefined);
    setNotice(undefined);

    if (!currentPassword || !newPassword || !confirmPassword) {
      setError('All password fields are required.');
      return;
    }
    if (newPassword.length < 8) {
      setError('New password must be at least 8 characters.');
      return;
    }
    if (newPassword !== confirmPassword) {
      setError('New password and confirmation do not match.');
      return;
    }

    setSaving(true);
    try {
      await adminPostJson<void>(api as AdminApiContext, '/api/v1/auth/change-password', {
        current_password: currentPassword,
        new_password: newPassword,
      });
      login({ ...user, mustChangePassword: false });
      setNotice('Password updated successfully. Redirecting...');
      navigate('/dashboard', { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to change password');
    } finally {
      setSaving(false);
    }
  };

  return (
    <main style={{ display: 'grid', placeItems: 'center', minHeight: '100vh', background: 'var(--gradient)' }}>
      <form
        onSubmit={handleSubmit}
        style={{
          width: 'min(460px, 92vw)',
          background: 'var(--surface-panel)',
          borderRadius: '1.5rem',
          padding: '2rem',
          border: '1px solid var(--border-subtle)',
          boxShadow: 'var(--auth-shadow)',
        }}
      >
        <p className="section-title">Security checkpoint</p>
        <h2 style={{ marginTop: 0 }}>Change your password</h2>
        <p style={{ color: 'var(--muted)' }}>
          Your account requires a password change before continuing.
        </p>
        {error ? (
          <div className="error-banner" style={{ marginBottom: '0.9rem' }}>{error}</div>
        ) : null}
        {notice ? (
          <div className="muted" style={{ marginBottom: '0.9rem' }}>{notice}</div>
        ) : null}

        <label htmlFor="change-password-current" style={{ display: 'block', marginBottom: '0.35rem' }}>
          Current password
        </label>
        <input
          id="change-password-current"
          className="search-input"
          type="password"
          value={currentPassword}
          onChange={(e) => setCurrentPassword(e.target.value)}
          autoComplete="current-password"
        />

        <label htmlFor="change-password-new" style={{ display: 'block', marginTop: '0.8rem', marginBottom: '0.35rem' }}>
          New password
        </label>
        <input
          id="change-password-new"
          className="search-input"
          type="password"
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
          autoComplete="new-password"
        />

        <label htmlFor="change-password-confirm" style={{ display: 'block', marginTop: '0.8rem', marginBottom: '0.35rem' }}>
          Confirm new password
        </label>
        <input
          id="change-password-confirm"
          className="search-input"
          type="password"
          value={confirmPassword}
          onChange={(e) => setConfirmPassword(e.target.value)}
          autoComplete="new-password"
        />

        <button className="cta-button" type="submit" style={{ marginTop: '1rem' }} disabled={saving || !api.canCallApi}>
          {saving ? 'Updating...' : 'Update Password'}
        </button>
      </form>
    </main>
  );
};
