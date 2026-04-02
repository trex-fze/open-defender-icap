import { useEffect, useMemo, useState } from 'react';
import { PendingClassification, usePendingClassifications } from '../hooks/usePendingClassifications';
import { usePendingActions } from '../hooks/usePendingActions';
import { useTaxonomyData } from '../hooks/useTaxonomyData';

const ACTION_OPTIONS = ['Allow', 'Block', 'Warn', 'Monitor', 'Review'];
const RISK_OPTIONS = ['low', 'medium', 'high', 'critical'];

export const PendingClassificationsPage = () => {
  const { data, loading, error, isMock, refresh, canCallApi } = usePendingClassifications();
  const {
    data: taxonomy,
    loading: taxonomyLoading,
    error: taxonomyError,
    isMock: isTaxonomyMock,
    canCallApi: canCallTaxonomyApi,
  } = useTaxonomyData();
  const { manualUnblock, busyKey, error: actionError } = usePendingActions();
  const [selectedKey, setSelectedKey] = useState<string | undefined>();
  const [action, setAction] = useState('Allow');
  const [reason, setReason] = useState('Manual analyst decision');
  const [riskLevel, setRiskLevel] = useState('low');
  const [categoryId, setCategoryId] = useState('');
  const [subcategoryId, setSubcategoryId] = useState('');
  const [message, setMessage] = useState<string | undefined>();

  const selectedRecord = selectedKey ? data.find((item) => item.normalizedKey === selectedKey) : undefined;
  const enabledCategories = useMemo(
    () =>
      taxonomy.categories
        .filter((category) => category.enabled)
        .map((category) => ({
          ...category,
          subcategories: category.subcategories.filter((sub) => sub.enabled),
        }))
        .filter((category) => category.subcategories.length > 0),
    [taxonomy.categories],
  );
  const selectedCategory = enabledCategories.find((category) => category.id === categoryId);
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
      if (enabledCategories.some((category) => category.id === prev)) {
        return prev;
      }
      return enabledCategories[0]?.id ?? '';
    });
  }, [selectedRecord, enabledCategories]);

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
      await manualUnblock(selectedRecord.normalizedKey, {
        action,
        primary_category: selectedCategory.id,
        subcategory: selectedSubcategory.id,
        risk_level: riskLevel,
        confidence: 0.95,
        reason: reason.trim() || undefined,
      });
      setMessage(`Updated ${selectedRecord.normalizedKey} with action ${action}`);
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
          <p className="section-title">Manual Decision</p>
          <p style={{ marginTop: 0, color: 'var(--muted)' }}>{selectedRecord.normalizedKey}</p>
          <div className="layout-grid">
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Category</span>
              <select
                className="search-input"
                value={categoryId}
                onChange={(event) => setCategoryId(event.target.value)}
              >
                {enabledCategories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}
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
                    {sub.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Action</span>
              <select className="search-input" value={action} onChange={(event) => setAction(event.target.value)}>
                {ACTION_OPTIONS.map((item) => (
                  <option key={item} value={item}>
                    {item}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Risk</span>
              <select className="search-input" value={riskLevel} onChange={(event) => setRiskLevel(event.target.value)}>
                {RISK_OPTIONS.map((item) => (
                  <option key={item} value={item}>
                    {item}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Reason</span>
              <input className="search-input" value={reason} onChange={(event) => setReason(event.target.value)} />
            </label>
          </div>
          {taxonomyError || isTaxonomyMock || !enabledCategories.length ? (
            <p style={{ marginTop: '0.75rem', marginBottom: 0, color: '#ffcf7f' }}>
              Taxonomy is unavailable for manual unblock. Check Admin API taxonomy endpoint and try again.
            </p>
          ) : null}
          <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
            <button
              className="cta-button"
              disabled={!canSubmitManual || busyKey === selectedRecord.normalizedKey}
              onClick={submitManualDecision}
            >
              {busyKey === selectedRecord.normalizedKey ? 'Saving...' : 'Apply Decision'}
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
                          Manual Unblock
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
