import { ChangeEvent, FormEvent, useState } from 'react';
import { NavLink } from 'react-router-dom';
import { adminPostJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from '../hooks/useAdminApi';

type ExchangeAction = 'allow' | 'block';
type ImportMode = 'merge' | 'replace';

type InvalidLine = {
  line_number: number;
  value: string;
  error: string;
};

type ImportResponse = {
  action: ExchangeAction;
  mode: ImportMode;
  dry_run: boolean;
  total_lines: number;
  parsed: number;
  duplicates: number;
  imported: number;
  updated: number;
  deleted: number;
  skipped: number;
  invalid: number;
  invalid_lines: InvalidLine[];
};

const downloadTextFile = (filename: string, content: string) => {
  const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
  const href = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = href;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(href);
};

const formatActionLabel = (action: ExchangeAction) =>
  action === 'allow' ? 'Allow' : 'Deny';

const parseUpload = (
  event: ChangeEvent<HTMLInputElement>,
  setText: (value: string) => void,
  setStatus: (value: string | undefined) => void,
  setError: (value: string | undefined) => void,
) => {
  const file = event.target.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => {
    const text = typeof reader.result === 'string' ? reader.result : '';
    setText(text);
    setStatus(`Loaded ${file.name}`);
    setError(undefined);
  };
  reader.onerror = () => setError('Failed to read selected file');
  reader.readAsText(file);
};

const parseErrorMessage = async (resp: Response): Promise<string> => {
  try {
    const body = await resp.json();
    if (typeof body?.message === 'string' && body.message.trim().length > 0) {
      return body.message;
    }
  } catch {
    // noop
  }
  return `Request failed (${resp.status})`;
};

const ExchangeSection = ({ action }: { action: ExchangeAction }) => {
  const api = useAdminApi();
  const [busy, setBusy] = useState<'export' | 'import'>();
  const [error, setError] = useState<string>();
  const [status, setStatus] = useState<string>();
  const [mode, setMode] = useState<ImportMode>('merge');
  const [dryRun, setDryRun] = useState(true);
  const [inputText, setInputText] = useState('');
  const [lastImport, setLastImport] = useState<ImportResponse>();

  const handleExport = async () => {
    if (!api.canCallApi) return;
    setBusy('export');
    setError(undefined);
    setStatus(undefined);
    try {
      const url = new URL('/api/v1/overrides/export', api.baseUrl);
      url.searchParams.set('action', action);
      const resp = await fetch(url.toString(), {
        method: 'GET',
        headers: api.headers,
      });
      if (!resp.ok) {
        throw new Error(await parseErrorMessage(resp));
      }
      const content = await resp.text();
      const filename = `overrides-${action}-${new Date().toISOString().replace(/[:.]/g, '-')}.txt`;
      downloadTextFile(filename, content);
      const lineCount = content.trim() ? content.split('\n').length : 0;
      setStatus(`Exported ${lineCount} ${formatActionLabel(action).toLowerCase()} records`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Export failed');
    } finally {
      setBusy(undefined);
    }
  };

  const handleImport = async (event: FormEvent) => {
    event.preventDefault();
    if (!api.canCallApi) return;

    if (!dryRun && mode === 'replace') {
      const confirmed = window.confirm(
        `${formatActionLabel(action)} replace will remove active ${formatActionLabel(
          action,
        ).toLowerCase()} overrides not listed in this file. Continue?`,
      );
      if (!confirmed) {
        return;
      }
    }

    setBusy('import');
    setError(undefined);
    setStatus(undefined);
    setLastImport(undefined);
    try {
      const response = await adminPostJson<ImportResponse>(
        api as AdminApiContext,
        '/api/v1/overrides/import',
        {
          action,
          mode,
          dry_run: dryRun,
          content: inputText,
        },
      );
      setLastImport(response);
      setStatus(
        `Import ${response.dry_run ? 'previewed' : 'completed'}: imported ${response.imported}, updated ${response.updated}, deleted ${response.deleted}, invalid ${response.invalid}`,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Import failed');
    } finally {
      setBusy(undefined);
    }
  };

  return (
    <section className="diagnostics-section">
      <h3 style={{ marginTop: 0 }}>{formatActionLabel(action)} Exchange</h3>
      <p className="muted" style={{ marginTop: 0 }}>
        Export and import {formatActionLabel(action).toLowerCase()} overrides as one-domain-per-line text files.
      </p>

      <div style={{ display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
        <button className="cta-button" onClick={handleExport} disabled={!api.canCallApi || busy === 'export'}>
          {busy === 'export' ? `Exporting ${formatActionLabel(action)}...` : `Export ${formatActionLabel(action)}`}
        </button>
      </div>

      <form onSubmit={handleImport} style={{ marginTop: '1rem', display: 'grid', gap: '0.75rem' }}>
        <label>
          <span style={{ display: 'block', marginBottom: '0.35rem' }}>Upload text file</span>
          <input
            className="search-input"
            type="file"
            accept=".txt,text/plain"
            onChange={(event) => parseUpload(event, setInputText, setStatus, setError)}
          />
        </label>

        <label>
          <span style={{ display: 'block', marginBottom: '0.35rem' }}>Or paste line-by-line entries</span>
          <textarea
            className="search-input"
            rows={8}
            placeholder={'# one domain per line\nexample.com\n*.example.org'}
            value={inputText}
            onChange={(event) => setInputText(event.target.value)}
          />
        </label>

        <div className="iam-role-checkboxes">
          <label className="checkbox-pill">
            <input
              type="radio"
              name={`mode-${action}`}
              checked={mode === 'merge'}
              onChange={() => setMode('merge')}
            />
            Merge
          </label>
          <label className="checkbox-pill">
            <input
              type="radio"
              name={`mode-${action}`}
              checked={mode === 'replace'}
              onChange={() => setMode('replace')}
            />
            Replace ({formatActionLabel(action)} only)
          </label>
          <label className="checkbox-pill">
            <input type="checkbox" checked={dryRun} onChange={(event) => setDryRun(event.target.checked)} />
            Dry run
          </label>
        </div>

        <div style={{ display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
          <button className="cta-button" type="submit" disabled={!api.canCallApi || busy === 'import'}>
            {busy === 'import' ? 'Importing...' : `Import ${formatActionLabel(action)}`}
          </button>
          <button
            className="cta-button btn-secondary"
            type="button"
            onClick={() => {
              setInputText('');
              setLastImport(undefined);
              setStatus(undefined);
              setError(undefined);
            }}
          >
            Clear
          </button>
        </div>
      </form>

      {error ? <p style={{ color: 'var(--status-error)' }}>{error}</p> : null}
      {status ? <p style={{ color: 'var(--status-success)' }}>{status}</p> : null}

      {lastImport ? (
        <div className="diagnostics-section" style={{ marginTop: '0.75rem' }}>
          <p className="section-title">Import Summary</p>
          <p className="muted" style={{ marginTop: 0 }}>
            Parsed {lastImport.parsed} entries from {lastImport.total_lines} lines ({lastImport.duplicates} duplicates ignored).
          </p>
          <div className="chip-row">
            <span className="chip chip--green">Imported {lastImport.imported}</span>
            <span className="chip chip--amber">Updated {lastImport.updated}</span>
            <span className="chip chip--amber">Skipped {lastImport.skipped}</span>
            <span className="chip chip--red">Deleted {lastImport.deleted}</span>
            <span className="chip chip--red">Invalid {lastImport.invalid}</span>
            <span className="chip subtle">mode: {lastImport.mode}</span>
            <span className="chip subtle">{lastImport.dry_run ? 'dry-run' : 'applied'}</span>
          </div>
          {lastImport.invalid_lines.length > 0 ? (
            <div className="table-wrapper" role="region" tabIndex={0} aria-label="Invalid import lines">
              <table>
                <thead>
                  <tr>
                    <th>Line</th>
                    <th>Value</th>
                    <th>Error</th>
                  </tr>
                </thead>
                <tbody>
                  {lastImport.invalid_lines.map((line) => (
                    <tr key={`${line.line_number}-${line.value}`}>
                      <td>{line.line_number}</td>
                      <td><code>{line.value}</code></td>
                      <td>{line.error}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
};

export const SettingsOverridesExchangePage = () => {
  return (
    <div className="settings-shell">
      <div className="page-header" style={{ marginBottom: '1.5rem' }}>
        <div>
          <p className="section-title">Settings</p>
          <h2 style={{ margin: 0 }}>Allow / Deny Exchange</h2>
          <p style={{ color: 'var(--muted)', marginTop: '0.35rem' }}>
            Manage portable line-by-line imports and exports for domain allow/deny overrides.
          </p>
          <p style={{ color: 'var(--muted)', marginTop: '0.35rem' }}>
            Exact scopes are mutually exclusive between Allow and Deny. Importing or creating the opposite action
            for the same domain replaces the current active action.
          </p>
        </div>
      </div>

      <div className="glass-panel" style={{ paddingBottom: 0 }}>
        <nav className="iam-tabs" style={{ marginBottom: '1rem' }}>
          <NavLink to="/settings/iam" className={({ isActive }) => `iam-tab ${isActive ? 'iam-tab--active' : ''}`}>
            IAM Workspace
          </NavLink>
          <NavLink
            to="/settings/classifications"
            className={({ isActive }) => `iam-tab ${isActive ? 'iam-tab--active' : ''}`}
          >
            Classifications Exchange
          </NavLink>
          <NavLink
            to="/settings/allow-deny-exchange"
            className={({ isActive }) => `iam-tab ${isActive ? 'iam-tab--active' : ''}`}
          >
            Allow / Deny Exchange
          </NavLink>
        </nav>

        <div className="iam-panel" style={{ display: 'grid', gap: '1rem' }}>
          <ExchangeSection action="allow" />
          <ExchangeSection action="block" />
        </div>
      </div>
    </div>
  );
};
