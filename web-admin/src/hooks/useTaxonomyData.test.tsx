import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useTaxonomyData } from './useTaxonomyData';

const mockFetch = vi.fn();
const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true,
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub,
}));

describe('useTaxonomyData', () => {
  const createWrapper = () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    return ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };

  beforeEach(() => {
    mockFetch.mockReset();
    global.fetch = mockFetch as unknown as typeof fetch;
  });

  it('sorts categories alphabetically and keeps unknown last', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({
        version: 'canon-v1',
        categories: [
          {
            id: 'unknown-unclassified',
            name: 'Unknown / Unclassified',
            enabled: true,
            locked: false,
            subcategories: [{ id: 'insufficient-evidence', name: 'Insufficient evidence', enabled: true, locked: false }],
          },
          {
            id: 'news-general',
            name: 'News',
            enabled: true,
            locked: false,
            subcategories: [{ id: 'general-news', name: 'General News', enabled: true, locked: false }],
          },
          {
            id: 'advertisements',
            name: 'Advertisements',
            enabled: true,
            locked: false,
            subcategories: [{ id: 'general-advertising', name: 'General advertising', enabled: true, locked: false }],
          },
        ],
      }),
    });

    const { result } = renderHook(() => useTaxonomyData(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.data.categories.map((category) => category.id)).toEqual([
      'advertisements',
      'news-general',
      'unknown-unclassified',
    ]);
  });
});
