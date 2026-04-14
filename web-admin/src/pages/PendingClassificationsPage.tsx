import { useEffect, useMemo, useState } from 'react';
import { PaginationControls } from '../components/PaginationControls';
import { PendingClassification, usePendingClassifications } from '../hooks/usePendingClassifications';
import { useLlmProviders } from '../hooks/useLlmProviders';
import { usePendingActions } from '../hooks/usePendingActions';
import { useTaxonomyData } from '../hooks/useTaxonomyData';

export const PendingClassificationsPage = () => {
  const [limit, setLimit] = useState(50);
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const { data, meta, loading, error, isMock, refresh, canCallApi } = usePendingClassifications(undefined, cursor, limit);
  const paginationMeta = meta ?? { has_more: false, next_cursor: undefined };
  const {
    data: taxonomy,
    loading: taxonomyLoading,
    error: taxonomyError,
    isMock: isTaxonomyMock,
    canCallApi: canCallTaxonomyApi,
  } = useTaxonomyData();
  const { manualClassify, metadataClassify, clearPending, clearAllPending, busyKey, busyAll, error: actionError } = usePendingActions();
  const {
    data: llmProviders,
    loading: llmProvidersLoading,
    error: llmProvidersError,
  } = useLlmProviders();
  const [selectedKey, setSelectedKey] = useState<string | undefined>();
  const [metadataKey, setMetadataKey] = useState<string | undefined>();
  const [reason, setReason] = useState('Manual analyst classification');
  const [metadataReason, setMetadataReason] = useState('Manual metadata-only classification from Pending Sites');
  const [categoryId, setCategoryId] = useState('');
  const [subcategoryId, setSubcategoryId] = useState('');
  const [providerName, setProviderName] = useState('');
  const [message, setMessage] = useState<string | undefined>();

  const selectedRecord = selectedKey ? data.find((item) => item.normalizedKey === selectedKey) : undefined;
  const metadataRecord = metadataKey ? data.find((item) => item.normalizedKey === metadataKey) : undefined;
  const taxonomyCategories = useMemo(
    () =>
      taxonomy.categories
        .map((category) => ({
          ...category,
          subcategories: category.subcategories,
        }))
        .filter((category) => category.subcategories.length > 0),
    [taxonomy.categories],
  );
  const selectedCategory = taxonomyCategories.find((category) => category.id === categoryId);
  const selectedSubcategory = selectedCategory?.subcategories.find((sub) => sub.id === subcategoryId);
  const canSubmitManual =
    Boolean(selectedCategory && selectedSubcategory) &&
    canCallApi &&
    canCallTaxonomyApi &&
    !isMock &&
    !isTaxonomyMock &&
    !taxonomyLoading;
  const canSubmitMetadata =
    Boolean(metadataRecord) &&
    Boolean(providerName) &&
    canCallApi &&
    !isMock &&
    !llmProvidersLoading &&
    llmProviders.length > 0;

  useEffect(() => {
    setCursor(undefined);
    setCursorStack([]);
  }, [limit]);

  useEffect(() => {
    if (!selectedRecord) {
      setCategoryId('');
      setSubcategoryId('');
      return;
    }

    setCategoryId((prev) => {
      if (taxonomyCategories.some((category) => category.id === prev)) {
        return prev;
      }
      return taxonomyCategories[0]?.id ?? '';
    });
  }, [selectedRecord, taxonomyCategories]);

  useEffect(() => {
    if (!selectedCategory) {
      setSubcategoryId('');
      return;
    }

    setSubcategoryId((prev) => {
      if (selectedCategory.subcategories.some((sub) => sub.id === prev)) {
        return prev;
      }
      return selectedCategory.subcategories[0]?.id ?? '';
    });
  }, [selectedCategory]);

  useEffect(() => {
    if (llmProviders.length === 0) {
      setProviderName('');
      return;
    }

    setProviderName((prev) => {
      if (llmProviders.some((provider) => provider.name === prev)) {
        return prev;
      }
      return llmProviders[0]?.name ?? '';
    });
  }, [llmProviders]);

  const submitManualDecision = async () => {
    if (!selectedRecord || !selectedCategory || !selectedSubcategory) return;
    setMessage(undefined);
    try {
      await manualClassify(selectedRecord.normalizedKey, {
        primary_category: selectedCategory.id,
        subcategory: selectedSubcategory.id,
        reason: reason.trim() || undefined,
      });
      setMessage(`Saved classification for ${selectedRecord.normalizedKey}`);
      setSelectedKey(undefined);
      setMetadataKey(undefined);
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  const submitMetadataDecision = async () => {
    if (!metadataRecord || !providerName) return;
    setMessage(undefined);
    try {
      await metadataClassify(metadataRecord.normalizedKey, {
        provider_name: providerName,
        reason: metadataReason.trim() || undefined,
      });
      setMessage(`Queued metadata-only classification for ${metadataRecord.normalizedKey}`);
      setMetadataKey(undefined);
      setSelectedKey(undefined);
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  const deletePendingRow = async (row: PendingClassification) => {
    if (!window.confirm(`Delete pending site ${row.normalizedKey}?`)) return;
    setMessage(undefined);
    try {
      await clearPending(row.normalizedKey);
      if (selectedKey === row.normalizedKey) {
        setSelectedKey(undefined);
      }
      if (metadataKey === row.normalizedKey) {
        setMetadataKey(undefined);
      }
      setMessage(`Deleted pending site ${row.normalizedKey}`);
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  const deleteAllPending = async () => {
    const phrase = window.prompt('Type DELETE ALL to remove every pending site record.');
    if (phrase !== 'DELETE ALL') {
      setMessage('Delete all canceled: confirmation phrase mismatch.');
      return;
    }
    setMessage(undefined);
    try {
      const deleted = await clearAllPending();
      setSelectedKey(undefined);
      setMetadataKey(undefined);
      setMessage(`Deleted ${deleted} pending sites.`);
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Pending Sites</p>
          <h2 style={{ margin: 0 }}>Content-first classification queue</h2>
        </div>
        <div className="page-header-actions">
          <button className="cta-button" onClick={refresh} disabled={loading}>
            Refresh
          </button>
          <button
            className="cta-button btn-danger-strong"
            onClick={deleteAllPending}
            disabled={loading || busyAll || isMock || !canCallApi || data.length === 0}
          >
            {busyAll ? 'Deleting...' : 'Delete All Pending'}
          </button>
        </div>
      </div>

      {error ? (
        <div className="glass-panel glass-panel--error">
          <p style={{ margin: 0, color: 'var(--status-error)' }}>Failed to load pending sites: {error}</p>
        </div>
      ) : null}

      {actionError ? (
        <div className="glass-panel glass-panel--error">
          <p style={{ margin: 0, color: 'var(--status-error)' }}>Pending action failed: {actionError}</p>
        </div>
      ) : null}

      {message ? (
        <div className="glass-panel glass-panel--success">
          <p style={{ margin: 0, color: 'var(--status-success)' }}>{message}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: 'var(--citrus)', marginTop: '0.5rem' }}>
          Using mock data (Admin API offline)
        </p>
      ) : null}

      {selectedRecord ? (
        <div className="glass-panel">
          <p className="section-title">Manual Classification</p>
          <p style={{ marginTop: 0, color: 'var(--muted)' }}>{selectedRecord.normalizedKey}</p>
          <div className="layout-grid">
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Category</span>
              <select
                className="search-input"
                value={categoryId}
                onChange={(event) => setCategoryId(event.target.value)}
              >
                {taxonomyCategories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}{category.enabled ? '' : ' (disabled)'}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Subcategory</span>
              <select
                className="search-input"
                value={subcategoryId}
                onChange={(event) => setSubcategoryId(event.target.value)}
                disabled={!selectedCategory}
              >
                {(selectedCategory?.subcategories ?? []).map((sub) => (
                  <option key={sub.id} value={sub.id}>
                    {sub.name}{sub.enabled ? '' : ' (disabled)'}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Reason</span>
              <input className="search-input" value={reason} onChange={(event) => setReason(event.target.value)} />
            </label>
          </div>
          <p style={{ marginTop: '0.75rem', marginBottom: 0, color: 'var(--muted)' }}>
            Classification only: enforcement overrides belong in the Allow / Deny list.
          </p>
          {taxonomyError || isTaxonomyMock || !taxonomyCategories.length ? (
            <p style={{ marginTop: '0.75rem', marginBottom: 0, color: 'var(--status-warning)' }}>
              Taxonomy is unavailable for manual classification. Check Admin API taxonomy endpoint and try again.
            </p>
          ) : null}
          <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
            <button
              className="cta-button"
              disabled={!canSubmitManual || busyKey === selectedRecord.normalizedKey}
              onClick={submitManualDecision}
            >
              {busyKey === selectedRecord.normalizedKey ? 'Saving...' : 'Save Classification'}
            </button>
            <button
              className="cta-button btn-secondary"
              onClick={() => setSelectedKey(undefined)}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}

      {metadataRecord ? (
        <div className="glass-panel">
          <p className="section-title">Metadata-only Classification</p>
          <p style={{ marginTop: 0, color: 'var(--muted)' }}>{metadataRecord.normalizedKey}</p>
          <div className="layout-grid">
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Preferred Provider</span>
              <select
                className="search-input"
                value={providerName}
                onChange={(event) => setProviderName(event.target.value)}
                disabled={llmProvidersLoading || llmProviders.length === 0}
              >
                {llmProviders.map((provider) => (
                  <option key={provider.name} value={provider.name}>
                    {provider.name} ({provider.healthStatus})
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Reason</span>
              <input
                className="search-input"
                value={metadataReason}
                onChange={(event) => setMetadataReason(event.target.value)}
              />
            </label>
          </div>
          <p style={{ marginTop: '0.75rem', marginBottom: 0, color: 'var(--muted)' }}>
            Preferred provider is tried first, then normal fallback policy applies if needed.
          </p>
          {llmProvidersError ? (
            <p style={{ marginTop: '0.75rem', marginBottom: 0, color: 'var(--status-warning)' }}>
              LLM providers are unavailable: {llmProvidersError}
            </p>
          ) : null}
          {!llmProvidersLoading && llmProviders.length === 0 ? (
            <p style={{ marginTop: '0.75rem', marginBottom: 0, color: 'var(--status-warning)' }}>
              No LLM providers are available for metadata-only classification.
            </p>
          ) : null}
          <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
            <button
              className="cta-button"
              disabled={!canSubmitMetadata || busyKey === metadataRecord.normalizedKey}
              onClick={submitMetadataDecision}
            >
              {busyKey === metadataRecord.normalizedKey ? 'Queueing...' : 'Queue Metadata-only Classification'}
            </button>
            <button
              className="cta-button btn-secondary"
              onClick={() => setMetadataKey(undefined)}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}

      <div className="glass-panel scroll-table-panel">
        <PaginationControls
          limit={limit}
          loading={loading}
          hasMore={Boolean(paginationMeta.next_cursor) && paginationMeta.has_more}
          canGoBack={Boolean(paginationMeta.prev_cursor) || cursorStack.length > 0}
          onPrev={() => {
            if (paginationMeta.prev_cursor) {
              setCursor(paginationMeta.prev_cursor);
              return;
            }
            setCursorStack((prev) => {
              if (prev.length === 0) return prev;
              const next = [...prev];
              const previousCursor = next.pop();
              setCursor(previousCursor || undefined);
              return next;
            });
          }}
          onNext={() => {
            if (!paginationMeta.next_cursor) return;
            setCursorStack((prev) => [...prev, cursor ?? '']);
            setCursor(paginationMeta.next_cursor);
          }}
          onLimitChange={setLimit}
        />
        {loading ? (
          <div>
            {Array.from({ length: 5 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : (
          <div className="table-wrapper scroll-table-region" role="region" tabIndex={0} aria-label="Pending classifications table">
            <table>
              <thead>
                <tr>
                  <th>Key</th>
                  <th>Status</th>
                  <th>Base URL</th>
                  <th>Updated</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {data.length === 0 ? (
                  <tr>
                    <td colSpan={5} style={{ textAlign: 'center', color: 'var(--muted)' }}>
                      No pending sites.
                    </td>
                  </tr>
                ) : (
                  data.map((item) => (
                    <tr key={item.normalizedKey}>
                      <td>{item.normalizedKey}</td>
                      <td>{item.status}</td>
                      <td>{item.baseUrl ?? '—'}</td>
                      <td>{item.updatedAt}</td>
                      <td style={{ textAlign: 'right' }}>
                        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.45rem', flexWrap: 'wrap' }}>
                          <button
                            className="cta-button"
                            style={{ padding: '0.4rem 0.8rem', fontSize: '0.75rem' }}
                            onClick={() => {
                              setSelectedKey(item.normalizedKey);
                              setMetadataKey(undefined);
                            }}
                          >
                            Manual Classify
                          </button>
                          <button
                            className="cta-button btn-secondary"
                            style={{ padding: '0.4rem 0.8rem', fontSize: '0.75rem' }}
                            disabled={busyKey === item.normalizedKey || isMock || !canCallApi || busyAll}
                            onClick={() => {
                              setMetadataKey(item.normalizedKey);
                              setSelectedKey(undefined);
                            }}
                          >
                            Metadata-only Classify
                          </button>
                          <button
                            className="cta-button btn-danger-strong"
                            style={{
                              padding: '0.4rem 0.8rem',
                              fontSize: '0.75rem',
                            }}
                            disabled={busyKey === item.normalizedKey || isMock || !canCallApi || busyAll}
                            onClick={() => deletePendingRow(item)}
                          >
                            {busyKey === item.normalizedKey ? 'Deleting...' : 'Delete'}
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
