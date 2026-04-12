import { ChangeEvent, FormEvent, useMemo, useState } from 'react';
import { NavLink } from 'react-router-dom';
import { adminGetJson, adminPostJson, type AdminApiContext } from '../api/adminClient';
import { useAdminApi } from '../hooks/useAdminApi';

type BundleEntry = {
  normalized_key: string;
  primary_category: string;
  subcategory: string;
  risk_level?: string | null;
  recommended_action?: string | null;
  confidence?: number | null;
  status?: string | null;
  flags?: unknown;
};

type ClassificationBundle = {
  schema_version: string;
  exported_at: string;
  taxonomy_version: string;
  entries: BundleEntry[];
};

type ImportResponse = {
  mode: string;
  recompute_policy_fields: boolean;
  dry_run: boolean;
  total_entries: number;
  imported: number;
  updated: number;
  skipped: number;
  replaced_deleted: number;
  invalid: number;
  invalid_rows_filename?: string;
  invalid_rows_jsonl?: string;
  invalid_rows_truncated: boolean;
};

type FlushResponse = {
  scope: string;
  dry_run: boolean;
  matched: number;
  deleted: number;
  invalid_keys: string[];
};

const downloadTextFile = (filename: string, content: string, mimeType: string) => {
  const blob = new Blob([content], { type: mimeType });
  const href = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = href;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(href);
};

export const SettingsClassificationsPage = () => {
  const api = useAdminApi();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState<string>();
  const [status, setStatus] = useState<string>();

  const [importMode, setImportMode] = useState<'merge' | 'replace'>('merge');
  const [importDryRun, setImportDryRun] = useState(true);
  const [recomputePolicyFields, setRecomputePolicyFields] = useState(true);
  const [importText, setImportText] = useState('');
  const [lastImport, setLastImport] = useState<ImportResponse>();

  const [flushScope, setFlushScope] = useState<'all' | 'prefix' | 'keys'>('all');
  const [flushPrefix, setFlushPrefix] = useState('');
  const [flushKeysText, setFlushKeysText] = useState('');
  const [flushDryRun, setFlushDryRun] = useState(true);
  const [flushConfirm, setFlushConfirm] = useState('');
  const [lastFlush, setLastFlush] = useState<FlushResponse>();

  const flushKeys = useMemo(
    () =>
      flushKeysText
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.length > 0),
    [flushKeysText],
  );

  const parseBundleFromUpload = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = typeof reader.result === 'string' ? reader.result : '';
      setImportText(text);
      setStatus(`Loaded ${file.name}`);
      setError(undefined);
    };
    reader.onerror = () => setError('Failed reading selected file');
    reader.readAsText(file);
  };

  const handleExport = async () => {
    if (!api.canCallApi) return;
    setBusy('export');
    setError(undefined);
    setStatus(undefined);
    try {
      const bundle = await adminGetJson<ClassificationBundle>(
        api as AdminApiContext,
        '/api/v1/classifications/export',
      );
      const filename = `classifications-export-${new Date().toISOString().replace(/[:.]/g, '-')}.json`;
      downloadTextFile(filename, JSON.stringify(bundle, null, 2), 'application/json');
      setStatus(`Exported ${bundle.entries.length} classifications`);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Classification export failed');
    } finally {
      setBusy(undefined);
    }
  };

  const handleImport = async (event: FormEvent) => {
    event.preventDefault();
    if (!api.canCallApi) return;
    setBusy('import');
    setError(undefined);
    setStatus(undefined);
    setLastImport(undefined);

    let bundle: ClassificationBundle;
    try {
      bundle = JSON.parse(importText) as ClassificationBundle;
    } catch {
      setBusy(undefined);
      setError('Invalid bundle JSON');
      return;
    }

    if (!recomputePolicyFields) {
      const confirmed = window.confirm(
        'Recompute is disabled. Imported risk/action/confidence values will be trusted as-is. Continue?',
      );
      if (!confirmed) {
        setBusy(undefined);
        return;
      }
    }

    if (!importDryRun && importMode === 'replace') {
      const confirmed = window.confirm(
        'Replace mode will remove existing domain classifications not present in this bundle. Continue?',
      );
      if (!confirmed) {
        setBusy(undefined);
        return;
      }
    }

    try {
      const response = await adminPostJson<ImportResponse>(
        api as AdminApiContext,
        '/api/v1/classifications/import',
        {
          bundle,
          mode: importMode,
          recompute_policy_fields: recomputePolicyFields,
          dry_run: importDryRun,
        },
      );
      setLastImport(response);
      setStatus(
        `Import finished: imported ${response.imported}, updated ${response.updated}, invalid ${response.invalid}`,
      );
      if (response.invalid_rows_jsonl && response.invalid_rows_jsonl.trim().length > 0) {
        const filename =
          response.invalid_rows_filename ||
          `classification-import-invalid-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`;
        downloadTextFile(filename, response.invalid_rows_jsonl, 'application/x-ndjson');
      }
      if (response.invalid_rows_truncated) {
        setStatus((prev) =>
          `${prev ?? 'Import finished.'} Invalid-row JSONL output was truncated due to size limits.`,
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Classification import failed');
    } finally {
      setBusy(undefined);
    }
  };

  const handleFlush = async (event: FormEvent) => {
    event.preventDefault();
    if (!api.canCallApi) return;
    if (!flushDryRun && flushConfirm.trim() !== 'FLUSH') {
      setError('Type FLUSH to confirm destructive flush');
      return;
    }

    setBusy('flush');
    setError(undefined);
    setStatus(undefined);
    setLastFlush(undefined);

    const body: Record<string, unknown> = {
      scope: flushScope,
      dry_run: flushDryRun,
    };
    if (flushScope === 'prefix') {
      if (!flushPrefix.trim()) {
        setError('Prefix is required for prefix scope');
        setBusy(undefined);
        return;
      }
      body.prefix = flushPrefix;
    }
    if (flushScope === 'keys') {
      if (flushKeys.length === 0) {
        setError('Provide at least one key for keys scope');
        setBusy(undefined);
        return;
      }
      body.keys = flushKeys;
    }

    try {
      const response = await adminPostJson<FlushResponse>(
        api as AdminApiContext,
        '/api/v1/classifications/flush',
        body,
      );
      setLastFlush(response);
      setStatus(`Flush ${response.dry_run ? 'previewed' : 'completed'}: ${response.deleted} deleted`);
      if (response.invalid_keys.length > 0) {
        setStatus((prev) => `${prev ?? 'Flush completed.'} Ignored invalid keys: ${response.invalid_keys.join(', ')}`);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Classification flush failed');
    } finally {
      setBusy(undefined);
    }
  };

  return (
    <div className="settings-shell">
      <div className="page-header" style={{ marginBottom: '1.5rem' }}>
        <div>
          <p className="section-title">Settings</p>
          <h2 style={{ margin: 0 }}>Classification Exchange</h2>
          <p style={{ color: 'var(--muted)', marginTop: '0.35rem' }}>
            Export, import, and flush domain classifications for backup and community sharing.
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

        <section className="iam-panel">
          <div style={{ display: 'grid', gap: '1rem' }}>
            <section>
              <h3>Export</h3>
              <p className="muted">Download a portable JSON bundle of current domain classifications.</p>
              <button className="cta-button" onClick={handleExport} disabled={!api.canCallApi || busy === 'export'}>
                {busy === 'export' ? 'Exporting...' : 'Export Bundle'}
              </button>
            </section>

            <section>
              <h3>Import</h3>
              <p className="muted">Upload a bundle, preview in dry-run mode, and optionally recompute policy-derived fields.</p>
              <form className="iam-form" onSubmit={handleImport}>
                <label>
                  <span>Bundle file</span>
                  <input type="file" accept="application/json" onChange={parseBundleFromUpload} />
                </label>
                <label>
                  <span>Bundle JSON</span>
                  <textarea
                    value={importText}
                    onChange={(e) => setImportText(e.target.value)}
                    rows={10}
                    placeholder="Paste exported classification bundle JSON"
                  />
                </label>
                <div className="iam-form-grid">
                  <label>
                    <span>Mode</span>
                    <select value={importMode} onChange={(e) => setImportMode(e.target.value as 'merge' | 'replace')}>
                      <option value="merge">merge</option>
                      <option value="replace">replace</option>
                    </select>
                  </label>
                  <label>
                    <span>Dry run</span>
                    <input
                      type="checkbox"
                      checked={importDryRun}
                      onChange={(e) => setImportDryRun(e.target.checked)}
                    />
                  </label>
                  <label>
                    <span>Recompute policy fields</span>
                    <input
                      type="checkbox"
                      checked={recomputePolicyFields}
                      onChange={(e) => setRecomputePolicyFields(e.target.checked)}
                    />
                  </label>
                </div>
                {!recomputePolicyFields && (
                  <div className="error-banner" style={{ margin: 0 }}>
                    Recompute is disabled. Imported risk/action/confidence values will be trusted as-is.
                  </div>
                )}
                <button
                  className="cta-button exchange-action-button"
                  disabled={!api.canCallApi || busy === 'import' || !importText.trim()}
                >
                  {busy === 'import' ? 'Importing...' : importDryRun ? 'Preview Import' : 'Apply Import'}
                </button>
              </form>
              {lastImport && (
                <div className="muted" style={{ marginTop: '0.5rem' }}>
                  mode={lastImport.mode} imported={lastImport.imported} updated={lastImport.updated} invalid={lastImport.invalid}
                </div>
              )}
            </section>

            <section>
              <h3>Flush</h3>
              <p className="muted">Delete classifications by scope. Use dry-run first in production.</p>
              <form className="iam-form" onSubmit={handleFlush}>
                <label>
                  <span>Scope</span>
                  <select
                    className="exchange-scope-select"
                    value={flushScope}
                    onChange={(e) => setFlushScope(e.target.value as 'all' | 'prefix' | 'keys')}
                  >
                    <option value="all">all</option>
                    <option value="prefix">prefix</option>
                    <option value="keys">keys</option>
                  </select>
                </label>
                {flushScope === 'prefix' && (
                  <label>
                    <span>Prefix</span>
                    <input
                      value={flushPrefix}
                      onChange={(e) => setFlushPrefix(e.target.value)}
                      placeholder="domain:example"
                    />
                  </label>
                )}
                {flushScope === 'keys' && (
                  <label>
                    <span>Keys (one per line)</span>
                    <textarea
                      value={flushKeysText}
                      onChange={(e) => setFlushKeysText(e.target.value)}
                      rows={6}
                      placeholder={'domain:example.com\ndomain:example.org'}
                    />
                  </label>
                )}
                <label>
                  <span>Dry run</span>
                  <input type="checkbox" checked={flushDryRun} onChange={(e) => setFlushDryRun(e.target.checked)} />
                </label>
                {!flushDryRun && (
                  <label>
                    <span>Type FLUSH to confirm</span>
                    <input value={flushConfirm} onChange={(e) => setFlushConfirm(e.target.value)} />
                  </label>
                )}
                <button className="cta-button exchange-action-button" disabled={!api.canCallApi || busy === 'flush'}>
                  {busy === 'flush' ? 'Running...' : flushDryRun ? 'Preview Flush' : 'Apply Flush'}
                </button>
              </form>
              {lastFlush && (
                <div className="muted" style={{ marginTop: '0.5rem' }}>
                  scope={lastFlush.scope} matched={lastFlush.matched} deleted={lastFlush.deleted}
                </div>
              )}
            </section>
          </div>
          {error && <div className="error-banner" style={{ marginTop: '1rem' }}>{error}</div>}
          {status && <div className="muted" style={{ marginTop: '0.75rem' }}>{status}</div>}
        </section>
      </div>
    </div>
  );
};
