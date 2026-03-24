import { ReactNode } from 'react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ProtectedRoute } from './App';
import { AuthContext, Role, UserProfile } from './context/AuthContext';

const mockUser: UserProfile = {
  name: 'Casey Blue',
  email: 'casey@example.com',
  roles: ['policy-viewer']
};

const renderWithAuth = (value: any, ui: ReactNode) =>
  render(<AuthContext.Provider value={value}>{ui}</AuthContext.Provider>);

const baseContextValue = {
  user: mockUser,
  tokens: { accessToken: 'demo-token', expiresAt: Date.now() + 60_000 },
  authNotice: undefined,
  login: vi.fn(),
  logout: vi.fn(),
  clearAuthNotice: vi.fn(),
  hasRole: vi.fn().mockReturnValue(true),
  hasAnyRole: vi.fn().mockReturnValue(true),
  setTokens: vi.fn()
};

describe('ProtectedRoute', () => {
  it('redirects anonymous users to login', () => {
    const ctx = { ...baseContextValue, user: null, hasAnyRole: vi.fn().mockReturnValue(false) };
    renderWithAuth(
      ctx,
      <MemoryRouter initialEntries={['/secure']}>
        <Routes>
          <Route
            path="/secure"
            element={
              <ProtectedRoute>
                <div>secure</div>
              </ProtectedRoute>
            }
          />
          <Route path="/login" element={<div>login page</div>} />
        </Routes>
      </MemoryRouter>
    );
    expect(screen.getByText('login page')).toBeInTheDocument();
  });

  it('blocks users without required roles', () => {
    const ctx = {
      ...baseContextValue,
      hasAnyRole: vi.fn().mockReturnValue(false)
    };
    renderWithAuth(
      ctx,
      <MemoryRouter initialEntries={['/restricted']}>
        <Routes>
          <Route
            path="/restricted"
            element={
              <ProtectedRoute roles={['policy-admin' as Role]}>
                <div>secret</div>
              </ProtectedRoute>
            }
          />
        </Routes>
      </MemoryRouter>
    );
    expect(screen.getByText(/Insufficient permissions/)).toBeInTheDocument();
  });

  it('renders children when user is authorized', () => {
    renderWithAuth(
      baseContextValue,
      <MemoryRouter initialEntries={['/ok']}>
        <Routes>
          <Route
            path="/ok"
            element={
              <ProtectedRoute roles={['policy-viewer']}>
                <div>allowed</div>
              </ProtectedRoute>
            }
          />
        </Routes>
      </MemoryRouter>
    );
    expect(screen.getByText('allowed')).toBeInTheDocument();
  });
});
