import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import AppLayout from './AppLayout';
import { AuthContext } from '../context/AuthContext';

const authValue = {
  user: { name: 'Alex Chen', email: 'alex@example.com', roles: ['policy-admin', 'policy-viewer'] as const },
  tokens: { accessToken: 'demo-token', expiresAt: Date.now() + 60_000 },
  authNotice: undefined,
  login: vi.fn(),
  logout: vi.fn(),
  clearAuthNotice: vi.fn(),
  hasRole: vi.fn().mockReturnValue(true),
  hasAnyRole: vi.fn().mockReturnValue(true),
  setTokens: vi.fn(),
};

const renderLayout = (initialPath = '/dashboard') =>
  render(
    <AuthContext.Provider value={authValue as any}>
      <MemoryRouter initialEntries={[initialPath]}>
        <Routes>
          <Route element={<AppLayout />}>
            <Route path="/dashboard" element={<div>Dashboard content</div>} />
            <Route path="/reports" element={<div>Reports content</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </AuthContext.Provider>,
  );

const setViewportWidth = (width: number) => {
  Object.defineProperty(window, 'innerWidth', {
    writable: true,
    configurable: true,
    value: width,
  });
  window.dispatchEvent(new Event('resize'));
};

describe('AppLayout sidebar', () => {
  beforeEach(() => {
    window.localStorage.clear();
    setViewportWidth(1200);
  });

  it('collapses to icon rail and keeps navigation working', async () => {
    const { container } = renderLayout('/dashboard');

    fireEvent.click(screen.getByRole('button', { name: /collapse sidebar/i }));

    expect(container.querySelector('.app-shell')?.className).toContain('sidebar-collapsed');

    fireEvent.click(screen.getByLabelText('Reports'));
    expect(await screen.findByText('Reports content')).toBeInTheDocument();
  });

  it('opens and closes mobile drawer', () => {
    setViewportWidth(800);

    const { container } = renderLayout('/dashboard');

    fireEvent.click(screen.getByRole('button', { name: /show menu/i }));
    expect(container.querySelector('.app-shell')?.className).toContain('sidebar-open-mobile');

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(container.querySelector('.app-shell')?.className).not.toContain('sidebar-open-mobile');
  });
});
