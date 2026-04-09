import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePendingActions } from './usePendingActions';

const mockFetch = vi.fn();

const apiStub = {
  baseUrl: 'https://admin.test',
  headers: { Authorization: 'Bearer test' },
  canCallApi: true,
};

vi.mock('./useAdminApi', () => ({
  useAdminApi: () => apiStub,
}));

describe('usePendingActions', () => {
  beforeEach(() => {
    mockFetch.mockReset();
    global.fetch = mockFetch as unknown as typeof fetch;
  });

  it('posts manual classification payload to manual-classify endpoint', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({}),
      text: async () => '{}',
    });

    const { result } = renderHook(() => usePendingActions());

    await act(async () => {
      await result.current.manualClassify('domain:test.example', {
        primary_category: 'social-media',
        subcategory: 'photo-sharing',
        reason: 'validated by analyst',
      });
    });

    expect(mockFetch).toHaveBeenCalledOnce();
    expect(mockFetch.mock.calls[0]?.[0]).toContain('/api/v1/classifications/domain%3Atest.example/manual-classify');
    expect(result.current.error).toBeUndefined();
  });

  it('stores error when request fails', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({ message: 'db unavailable' }),
      text: async () => JSON.stringify({ message: 'db unavailable' }),
    });

    const { result } = renderHook(() => usePendingActions());

    await expect(
      act(async () => {
        await result.current.manualClassify('domain:test.example', {
          primary_category: 'social-media',
          subcategory: 'photo-sharing',
          reason: 'malicious payload',
        });
      }),
    ).rejects.toThrow(/db unavailable/i);

    expect(result.current.busyKey).toBeUndefined();
  });

  it('deletes a single pending row', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 204,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({}),
      text: async () => '',
    });

    const { result } = renderHook(() => usePendingActions());

    await act(async () => {
      await result.current.clearPending('domain:test.example');
    });

    expect(mockFetch).toHaveBeenCalledOnce();
    expect(mockFetch.mock.calls[0]?.[0]).toContain('/api/v1/classifications/domain%3Atest.example/pending');
  });

  it('deletes all pending rows and returns count', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      headers: new Headers({ 'Content-Type': 'application/json' }),
      json: async () => ({ deleted: 5 }),
      text: async () => JSON.stringify({ deleted: 5 }),
    });

    const { result } = renderHook(() => usePendingActions());

    let deleted = 0;
    await act(async () => {
      deleted = await result.current.clearAllPending();
    });

    expect(deleted).toBe(5);
    expect(mockFetch).toHaveBeenCalledOnce();
    expect(mockFetch.mock.calls[0]?.[0]).toContain('/api/v1/classifications/pending');
  });
});
