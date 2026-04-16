import { MemoryRouter } from 'react-router-dom';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SettingsOverridesExchangePage } from './SettingsOverridesExchangePage';

vi.mock('../hooks/useAdminApi', () => ({
  useAdminApi: () => ({
    baseUrl: 'http://localhost:19000',
    accessToken: 'demo-token',
    canCallApi: true,
    headers: {},
    onUnauthorized: vi.fn(),
  }),
}));

describe('SettingsOverridesExchangePage', () => {
  it('shows exact-scope exclusivity guidance', () => {
    render(
      <MemoryRouter>
        <SettingsOverridesExchangePage />
      </MemoryRouter>,
    );

    expect(
      screen.getByText(/Exact scopes are mutually exclusive between Allow and Deny/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/replaces the current active action/i),
    ).toBeInTheDocument();
  });
});
