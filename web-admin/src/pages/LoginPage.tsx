import { FormEvent, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '../context/AuthContext';
import { resolveAdminApiBase } from '../utils/adminApiBase';

type LoginResponse = {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  refresh_expires_in?: number;
  user: {
    username?: string | null;
    email: string;
    display_name?: string | null;
    roles: string[];
    must_change_password?: boolean;
  };
};

export const LoginPage = () => {
  const { login, authNotice, clearAuthNotice } = useAuth();
  const navigate = useNavigate();
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string>();

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    if (!username.trim() || !password) {
      return;
    }
    setError(undefined);
    setSubmitting(true);
    try {
      const url = new URL('/api/v1/auth/login', resolveAdminApiBase()).toString();
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username: username.trim(), password }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => null);
        const message =
          (body && typeof body.message === 'string' && body.message) ||
          (body && typeof body.error === 'string' && body.error) ||
          `Login failed (${response.status})`;
        throw new Error(message);
      }
      const payload = (await response.json()) as LoginResponse;
      login(
        {
          username: payload.user.username ?? undefined,
          email: payload.user.email,
          name: payload.user.display_name || payload.user.username || payload.user.email,
          mustChangePassword: payload.user.must_change_password === true,
          roles: payload.user.roles.filter(
            (role): role is 'policy-admin' | 'policy-editor' | 'policy-viewer' | 'auditor' =>
              role === 'policy-admin' ||
              role === 'policy-editor' ||
              role === 'policy-viewer' ||
              role === 'auditor',
          ),
        },
        {
          tokens: {
            accessToken: payload.access_token,
            refreshToken: payload.refresh_token,
            expiresAt: Date.now() + payload.expires_in * 1000,
          },
        },
      );
      if (payload.user.must_change_password) {
        navigate('/auth/change-password', { replace: true });
      } else {
        navigate('/dashboard', { replace: true });
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Login failed';
      setError(message);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <main style={{ display: 'grid', placeItems: 'center', minHeight: '100vh', background: 'var(--gradient)' }}>
      <form
        onSubmit={handleSubmit}
        style={{
          width: 'min(420px, 90vw)',
          background: 'var(--surface-panel)',
          borderRadius: '1.5rem',
          padding: '2rem',
          border: '1px solid var(--border-subtle)',
          boxShadow: 'var(--auth-shadow)',
        }}
      >
        <div className="login-brand-wrap">
          <img src="/brand/logo.png" alt="Open Defender ICAP" className="login-brand-logo" />
        </div>
        <p className="section-title">Local Sign-in</p>
        <h2 style={{ marginTop: 0 }}>Welcome back</h2>
        <p style={{ color: 'var(--muted)' }}>Sign in using your local username or email and password.</p>
        {authNotice ? (
          <div
            style={{
                border: '1px solid var(--error-border)',
                background: 'var(--error-bg)',
                borderRadius: '0.75rem',
                padding: '0.65rem 0.8rem',
                color: 'var(--status-error)',
                marginBottom: '0.9rem',
              }}
            >
            <p style={{ margin: 0 }}>{authNotice}</p>
          </div>
        ) : null}
        {error ? (
          <div
            style={{
                border: '1px solid var(--error-border)',
                background: 'var(--error-bg)',
                borderRadius: '0.75rem',
                padding: '0.65rem 0.8rem',
                color: 'var(--status-error)',
                marginBottom: '0.9rem',
              }}
            >
            <p style={{ margin: 0 }}>{error}</p>
          </div>
        ) : null}
        <label htmlFor="login-username" style={{ display: 'block', marginBottom: '0.35rem' }}>
          Username or Email
        </label>
        <input
          id="login-username"
          className="search-input"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          type="text"
        />
        <label htmlFor="login-password" style={{ display: 'block', marginTop: '0.8rem', marginBottom: '0.35rem' }}>
          Password
        </label>
        <input
          id="login-password"
          className="search-input"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          type="password"
        />
        <button type="submit" className="cta-button" style={{ width: '100%', marginTop: '1.25rem' }} disabled={submitting}>
          Continue
        </button>
        {authNotice ? (
          <button
            type="button"
            className="cta-button btn-secondary"
            style={{ width: '100%', marginTop: '0.6rem' }}
            onClick={clearAuthNotice}
          >
            Dismiss notice
          </button>
        ) : null}
      </form>
    </main>
  );
};
