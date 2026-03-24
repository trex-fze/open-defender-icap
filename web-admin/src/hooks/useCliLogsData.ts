import { useState } from 'react';
import { adminGetJson, type AdminApiContext } from '../api/adminClient';
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
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | undefined>();

  const fetchLogs = async (operatorId?: string) => {
    setLoading(true);
    setError(undefined);
    try {
      const data = await adminGetJson<CliLogRecord[]>(
        api as AdminApiContext,
        '/api/v1/cli-logs',
        {
          operator_id: operatorId?.trim() || undefined,
          limit: 100,
        },
      );
      setLogs(data);
    } catch (err) {
      setLogs([]);
      setError(err instanceof Error ? err.message : 'Failed to load CLI logs');
    } finally {
      setLoading(false);
    }
  };

  return {
    logs,
    fetchLogs,
    loading,
    error,
    canCallApi: api.canCallApi,
  } as const;
};
