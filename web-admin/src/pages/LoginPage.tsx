import { FormEvent, useState } from 'react';
import { useAuth } from '../context/AuthContext';

const BOOTSTRAP_TOKEN = (import.meta.env.VITE_ADMIN_TOKEN ?? '').trim();

export const LoginPage = () => {
  const { login, authNotice, clearAuthNotice } = useAuth();
  const [email, setEmail] = useState('avery@example.com');

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    const fallbackToken = BOOTSTRAP_TOKEN || `demo-${crypto.randomUUID?.() ?? Date.now()}`;
    login(
      { email, name: email.split('@')[0] },
      {
        tokens: {
          accessToken: fallbackToken,
          expiresAt: Date.now() + 60 * 60 * 1000,
        },
      },
    );
  };

  return (
    <main style={{ display: 'grid', placeItems: 'center', minHeight: '100vh', background: 'var(--gradient)' }}>
      <form
        onSubmit={handleSubmit}
        style={{
          width: 'min(420px, 90vw)',
          background: 'rgba(4, 10, 24, 0.75)',
          borderRadius: '1.5rem',
          padding: '2rem',
          border: '1px solid rgba(255,255,255,0.08)',
          boxShadow: '0 30px 80px rgba(0,0,0,0.45)',
        }}
      >
        <p className="section-title">OIDC Sign-in</p>
        <h2 style={{ marginTop: 0 }}>Welcome back</h2>
        <p style={{ color: '#8ca0cb' }}>Prototype device flow — in production this will redirect to your IdP.</p>
        {authNotice ? (
          <div
            style={{
              border: '1px solid rgba(255, 122, 122, 0.5)',
              borderRadius: '0.75rem',
              padding: '0.65rem 0.8rem',
              color: '#ff9b9b',
              marginBottom: '0.9rem',
            }}
          >
            <p style={{ margin: 0 }}>{authNotice}</p>
          </div>
        ) : null}
        <label htmlFor="login-email" style={{ display: 'block', marginBottom: '0.35rem' }}>
          Email
        </label>
        <input
          id="login-email"
          className="search-input"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          type="email"
        />
        <button type="submit" className="cta-button" style={{ width: '100%', marginTop: '1.25rem' }}>
          Continue
        </button>
        {authNotice ? (
          <button
            type="button"
            className="cta-button"
            style={{ width: '100%', marginTop: '0.6rem', background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
            onClick={clearAuthNotice}
          >
            Dismiss notice
          </button>
        ) : null}
      </form>
    </main>
  );
};
