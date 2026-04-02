import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useClassificationActions } from './useClassificationActions';

const mockFetch = vi.fn();

const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true,
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub,
}));

describe('useClassificationActions', () => {
  beforeEach(() => {
    mockFetch.mockReset();
    global.fetch = mockFetch as unknown as typeof fetch;
  });

  it('patches classification update payload', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({}),
    });

    const { result } = renderHook(() => useClassificationActions());

    await act(async () => {
      await result.current.updateClassification('domain:test.example', {
        primary_category: 'social-media',
        subcategory: 'social-networking',
      });
    });

    expect(mockFetch).toHaveBeenCalledOnce();
    expect(mockFetch.mock.calls[0]?.[0]).toContain('/api/v1/classifications/domain%3Atest.example');
    expect(mockFetch.mock.calls[0]?.[1]).toMatchObject({ method: 'PATCH' });
  });

  it('deletes classification key', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 204,
      json: async () => ({}),
    });

    const { result } = renderHook(() => useClassificationActions());

    await act(async () => {
      await result.current.deleteClassification('domain:test.example');
    });

    expect(mockFetch).toHaveBeenCalledOnce();
    expect(mockFetch.mock.calls[0]?.[0]).toContain('/api/v1/classifications/domain%3Atest.example');
    expect(mockFetch.mock.calls[0]?.[1]).toMatchObject({ method: 'DELETE' });
  });
});
