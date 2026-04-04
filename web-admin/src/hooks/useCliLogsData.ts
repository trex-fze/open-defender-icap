import { useState } from 'react';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
import type { CursorMeta, CursorPaged } from '../types/pagination';
import { useAdminApi } from './useAdminApi';

export type CliLogRecord = {
  id: string;
  operator_id?: string;
  command: string;
  args_hash?: string;
  result?: string;
  created_at: string;
};

export const useCliLogsData = () => {
  const api = useAdminApi();
  const [logs, setLogs] = useState<CliLogRecord[]>([]);
  const [meta, setMeta] = useState<CursorMeta>({ limit: 50, has_more: false });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const fetchLogs = async (operatorId?: string, cursor?: string, limit = 100) => {
    setLoading(true);
    setError(undefined);
    try {
      const body = await adminGetJson<CursorPaged<CliLogRecord>>(
        api as AdminApiContext,
        '/api/v1/cli-logs',
        {
          operator_id: operatorId?.trim() || undefined,
          limit,
          cursor,
        },
      );
      setLogs(body.data);
      setMeta(body.meta);
    } catch (err) {
      setLogs([]);
      setMeta({ limit, has_more: false });
      setError(err instanceof Error ? err.message : 'Failed to load CLI logs');
    } finally {
      setLoading(false);
    }
  };

  return {
    logs,
    meta,
    fetchLogs,
    loading,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
