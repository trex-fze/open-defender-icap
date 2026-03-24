import { useState } from 'react';
import { adminPostJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from './useAdminApi';

type ResolveReviewPayload = {
  status: string;
  decision_action?: string;
  decision_notes?: string;
};

export const useReviewQueueActions = () => {
  const api = useAdminApi();
  const [resolvingId, setResolvingId] = useState<string | undefined>();
  const [error, setError] = useState<string | undefined>();

  const resolveReview = async (
    id: string,
    payload: ResolveReviewPayload,
  ): Promise<void> => {
    setResolvingId(id);
    setError(undefined);
    try {
      await adminPostJson<unknown, ResolveReviewPayload>(
        api as AdminApiContext,
        `/api/v1/review-queue/${id}/resolve`,
        payload,
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to resolve review item';
      setError(message);
      throw err;
    } finally {
      setResolvingId(undefined);
    }
  };

  return {
    resolveReview,
    resolvingId,
    error,
    canCallApi: api.canCallApi,
  };
};
