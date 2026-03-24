import { FormEvent, useState } from 'react';
import { rbacMatrix } from '../data/mockData';
import { useCliLogsData } from '../hooks/useCliLogsData';

export const SettingsRbacPage = () => {
  const { logs, fetchLogs, loading, error, canCallApi } = useCliLogsData();
  const [operatorId, setOperatorId] = useState('');

  const onLoadLogs = async (event: FormEvent) => {
    event.preventDefault();
    await fetchLogs(operatorId);
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">RBAC</p>
          <h2 style={{ margin: 0 }}>Role assignments</h2>
        </div>
      </div>

      <form className="glass-panel" onSubmit={onLoadLogs}>
        <p className="section-title">CLI Audit Logs</p>
        <label>
          <span style={{ display: 'block', marginBottom: '0.35rem' }}>Operator filter (optional)</span>
          <input
            className="search-input"
            value={operatorId}
            onChange={(event) => setOperatorId(event.target.value)}
            placeholder="alice@example.com"
          />
        </label>
        <div style={{ marginTop: '1rem' }}>
          <button className="cta-button" disabled={!canCallApi || loading}>
            {loading ? 'Loading...' : 'Load Logs'}
          </button>
        </div>
      </form>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>{error}</p>
        </div>
      ) : null}

      {logs.length > 0 ? (
        <div className="glass-panel">
          <p className="section-title">Recent CLI Operations</p>
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="CLI logs table">
            <table>
              <thead>
                <tr>
                  <th>When</th>
                  <th>Operator</th>
                  <th>Command</th>
                  <th>Result</th>
                </tr>
              </thead>
              <tbody>
                {logs.map((row) => (
                  <tr key={row.id}>
                    <td>{new Date(row.created_at).toLocaleString()}</td>
                    <td>{row.operator_id ?? 'unknown'}</td>
                    <td>{row.command}</td>
                    <td>{row.result ?? 'ok'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}

      <div className="glass-panel">
        <div className="table-wrapper" role="region" tabIndex={0} aria-label="RBAC matrix table">
          <table>
            <thead>
              <tr>
                <th>User</th>
                <th>Email</th>
                <th>Roles</th>
              </tr>
            </thead>
            <tbody>
              {rbacMatrix.map((user) => (
                <tr key={user.email}>
                  <td>{user.name}</td>
                  <td>{user.email}</td>
                  <td>
                    {user.roles.map((role) => (
                      <span key={role} className="chip" style={{ marginRight: '0.35rem' }}>
                        {role}
                      </span>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
