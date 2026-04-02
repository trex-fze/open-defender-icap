import { useEffect, useMemo, useState } from 'react';
import { PendingClassification, usePendingClassifications } from '../hooks/usePendingClassifications';
import { usePendingActions } from '../hooks/usePendingActions';
import { useTaxonomyData } from '../hooks/useTaxonomyData';

export const PendingClassificationsPage = () => {
  const { data, loading, error, isMock, refresh, canCallApi } = usePendingClassifications();
  const {
    data: taxonomy,
    loading: taxonomyLoading,
    error: taxonomyError,
    isMock: isTaxonomyMock,
    canCallApi: canCallTaxonomyApi,
  } = useTaxonomyData();
  const { manualClassify, busyKey, error: actionError } = usePendingActions();
  const [selectedKey, setSelectedKey] = useState<string | undefined>();
  const [reason, setReason] = useState('Manual analyst classification');
  const [categoryId, setCategoryId] = useState('');
  const [subcategoryId, setSubcategoryId] = useState('');
  const [message, setMessage] = useState<string | undefined>();

  const selectedRecord = selectedKey ? data.find((item) => item.normalizedKey === selectedKey) : undefined;
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
        <button className="cta-button" onClick={refresh} disabled={loading}>
          Refresh
        </button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load pending sites: {error}</p>
        </div>
      ) : null}

      {actionError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Manual decision failed: {actionError}</p>
        </div>
      ) : null}

      {message ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: '#9ef7eb' }}>{message}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: '#fdd744', marginTop: '0.5rem' }}>
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
            <p style={{ marginTop: '0.75rem', marginBottom: 0, color: '#ffcf7f' }}>
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
              className="cta-button"
              style={{ background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
              onClick={() => setSelectedKey(undefined)}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}

      <div className="glass-panel">
        {loading ? (
          <div>
            {Array.from({ length: 5 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : (
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Pending classifications table">
            <table>
              <thead>
                <tr>
                  <th>Key</th>
                  <th>Status</th>
                  <th>Base URL</th>
                  <th>Updated</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {data.length === 0 ? (
                  <tr>
                    <td colSpan={5} style={{ textAlign: 'center', color: '#7f8fb2' }}>
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
                        <button className="cta-button" style={{ padding: '0.4rem 0.8rem', fontSize: '0.75rem' }} onClick={() => setSelectedKey(item.normalizedKey)}>
                          Manual Classify
                        </button>
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
