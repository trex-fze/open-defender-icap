import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import { AuthContext } from '../context/AuthContext';
import { LoginPage } from './LoginPage';

const authValue = {
  user: null,
  tokens: null,
  authNotice: undefined,
  login: vi.fn(),
  logout: vi.fn(),
  clearAuthNotice: vi.fn(),
  expireSession: vi.fn(),
  hasRole: vi.fn().mockReturnValue(false),
  hasAnyRole: vi.fn().mockReturnValue(false),
  setTokens: vi.fn(),
};

describe('LoginPage', () => {
  it('renders company website link with safe new-tab attributes', () => {
    render(
      <AuthContext.Provider value={authValue as any}>
        <MemoryRouter>
          <LoginPage />
        </MemoryRouter>
      </AuthContext.Provider>,
    );

    const companyLink = screen.getByRole('link', { name: /open trex website/i });
    expect(companyLink).toHaveAttribute('href', 'https://trex.ae/');
    expect(companyLink).toHaveAttribute('target', '_blank');
    expect(companyLink).toHaveAttribute('rel', 'noopener noreferrer');
  });
});
