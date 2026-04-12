import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ThemeProvider, useTheme } from './ThemeContext';

const ThemeProbe = () => {
  const { preference, resolvedTheme, setPreference } = useTheme();
  return (
    <div>
      <p data-testid="preference">{preference}</p>
      <p data-testid="resolved">{resolvedTheme}</p>
      <button type="button" onClick={() => setPreference('light')}>light</button>
    </div>
  );
};

describe('ThemeContext', () => {
  it('defaults to system preference and resolves from media query', () => {
    window.matchMedia = vi.fn().mockImplementation((query: string) => ({
      matches: query.includes('dark'),
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));

    render(
      <ThemeProvider>
        <ThemeProbe />
      </ThemeProvider>,
    );

    expect(screen.getByTestId('preference')).toHaveTextContent('system');
    expect(screen.getByTestId('resolved')).toHaveTextContent('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
  });

  it('persists explicit theme preference', () => {
    render(
      <ThemeProvider>
        <ThemeProbe />
      </ThemeProvider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'light' }));

    expect(screen.getByTestId('preference')).toHaveTextContent('light');
    expect(screen.getByTestId('resolved')).toHaveTextContent('light');
    expect(window.localStorage.getItem('od.theme.preference')).toBe('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });
});
