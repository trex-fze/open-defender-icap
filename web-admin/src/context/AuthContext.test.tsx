import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AuthProvider, useAuth } from './AuthContext';

const Harness = () => {
  const { user, tokens, hasRole, login, logout, authNotice, clearAuthNotice } = useAuth();
  return (
    <div>
      <p data-testid="user-name">{user?.name ?? 'anonymous'}</p>
      <p data-testid="has-admin">{hasRole('policy-admin') ? 'yes' : 'no'}</p>
      <p data-testid="token">{tokens?.accessToken ?? 'none'}</p>
      <p data-testid="notice">{authNotice ?? 'none'}</p>
      <button
        type="button"
        onClick={() =>
          login({ name: 'Nova', roles: ['policy-editor'] }, { tokens: { accessToken: 'abc123' } })
        }
      >
        login
      </button>
      <button type="button" onClick={() => logout()}>
        logout
      </button>
      <button type="button" onClick={() => clearAuthNotice()}>
        clear-notice
      </button>
    </div>
  );
};

const renderWithProvider = () => render(<AuthProvider><Harness /></AuthProvider>);

describe('AuthProvider', () => {
  it('starts unauthenticated by default', () => {
    renderWithProvider();
    expect(screen.getByTestId('user-name')).toHaveTextContent('anonymous');
    expect(screen.getByTestId('has-admin')).toHaveTextContent('no');
  });

  it('updates user and tokens on login', () => {
    renderWithProvider();
    fireEvent.click(screen.getByText('login'));
    expect(screen.getByTestId('user-name')).toHaveTextContent('Nova');
    expect(screen.getByTestId('token')).toHaveTextContent('abc123');
    expect(screen.getByTestId('has-admin')).toHaveTextContent('no');
  });

  it('clears session on logout', () => {
    renderWithProvider();
    fireEvent.click(screen.getByText('logout'));
    expect(screen.getByTestId('user-name')).toHaveTextContent('anonymous');
    expect(screen.getByTestId('token')).toHaveTextContent('none');
  });
});
