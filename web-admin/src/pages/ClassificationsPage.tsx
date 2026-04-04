import { useEffect, useMemo, useState } from 'react';
import { PaginationControls } from '../components/PaginationControls';
import { useClassificationsData, type ClassificationStateFilter } from '../hooks/useClassificationsData';
import { useClassificationActions } from '../hooks/useClassificationActions';
import { useTaxonomyData } from '../hooks/useTaxonomyData';

const FILTERS: ClassificationStateFilter[] = ['all', 'classified', 'unclassified'];

export const ClassificationsPage = () => {
  const [stateFilter, setStateFilter] = useState<ClassificationStateFilter>('all');
  const [search, setSearch] = useState('');
  const [selectedKey, setSelectedKey] = useState<string | undefined>();
  const [categoryId, setCategoryId] = useState('');
  const [subcategoryId, setSubcategoryId] = useState('');
  const [reason, setReason] = useState('Operator classification correction');
  const [message, setMessage] = useState<string | undefined>();
  const [limit, setLimit] = useState(50);
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);

  const { data, meta, loading, error, refresh } = useClassificationsData(stateFilter, search, cursor, limit);
  const paginationMeta = meta ?? { has_more: false, next_cursor: undefined };
  const { updateClassification, deleteClassification, busyKey, error: actionError, canCallApi } =
    useClassificationActions();
  const { data: taxonomy, loading: taxonomyLoading, error: taxonomyError, isMock: taxonomyMock } = useTaxonomyData();

  const taxonomyCategories = useMemo(
    () =>
      taxonomy.categories
        .map((category) => ({ ...category, subcategories: category.subcategories }))
        .filter((category) => category.subcategories.length > 0),
    [taxonomy.categories],
  );
  const selectedRow = selectedKey ? data.find((item) => item.normalized_key === selectedKey) : undefined;
  const selectedCategory = taxonomyCategories.find((category) => category.id === categoryId);
  const selectedSubcategory = selectedCategory?.subcategories.find((sub) => sub.id === subcategoryId);

  useEffect(() => {
    setCursor(undefined);
    setCursorStack([]);
  }, [stateFilter, search, limit]);

  const canEdit = Boolean(selectedRow && selectedCategory && selectedSubcategory) && !taxonomyLoading && !taxonomyMock;

  const openEditor = (normalizedKey: string) => {
    setSelectedKey(normalizedKey);
    setMessage(undefined);
    const row = data.find((item) => item.normalized_key === normalizedKey);
    const initialCategory =
      taxonomyCategories.find((category) => category.id === row?.primary_category) ?? taxonomyCategories[0];
    const initialSubcategory =
      initialCategory?.subcategories.find((subcategory) => subcategory.id === row?.subcategory) ??
      initialCategory?.subcategories[0];
    setCategoryId(initialCategory?.id ?? '');
    setSubcategoryId(initialSubcategory?.id ?? '');
  };

  const onCategoryChange = (value: string) => {
    setCategoryId(value);
    const nextCategory = taxonomyCategories.find((category) => category.id === value);
    setSubcategoryId(nextCategory?.subcategories[0]?.id ?? '');
  };

  const saveClassification = async () => {
    if (!selectedRow || !selectedCategory || !selectedSubcategory) return;
    setMessage(undefined);
    await updateClassification(selectedRow.normalized_key, {
      primary_category: selectedCategory.id,
      subcategory: selectedSubcategory.id,
      reason: reason.trim() || undefined,
    });
    setMessage(`Updated classification for ${selectedRow.normalized_key}`);
    setSelectedKey(undefined);
    await refresh();
  };

  const removeClassification = async (normalizedKey: string) => {
    if (!confirm(`Remove classification state for ${normalizedKey}?`)) return;
    setMessage(undefined);
    await deleteClassification(normalizedKey);
    setMessage(`Removed ${normalizedKey} from classification lists`);
    if (selectedKey === normalizedKey) {
      setSelectedKey(undefined);
    }
    await refresh();
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Classifications</p>
          <h2 style={{ margin: 0 }}>Classified / Unclassified Sites</h2>
        </div>
        <button className="cta-button" onClick={() => refresh()} disabled={loading}>
          Refresh
        </button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load classification list: {error}</p>
        </div>
      ) : null}
      {actionError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Classification action failed: {actionError}</p>
        </div>
      ) : null}
      {message ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: '#9ef7eb' }}>{message}</p>
        </div>
      ) : null}

      <div className="glass-panel" style={{ marginBottom: '1rem' }}>
        <div style={{ display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
          <label style={{ minWidth: 220 }}>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>State</span>
            <select className="search-input" value={stateFilter} onChange={(event) => setStateFilter(event.target.value as ClassificationStateFilter)}>
              {FILTERS.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>
          <label style={{ flex: 1, minWidth: 280 }}>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>Search key/category</span>
            <input className="search-input" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="domain:tiktok.com or social-media" />
          </label>
        </div>
        <PaginationControls
          limit={limit}
          loading={loading}
          hasMore={Boolean(paginationMeta.next_cursor) && paginationMeta.has_more}
          canGoBack={cursorStack.length > 0}
          onPrev={() => {
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
          onLimitChange={(nextLimit) => setLimit(nextLimit)}
        />
      </div>

      {selectedRow ? (
        <div className="glass-panel" style={{ marginBottom: '1rem' }}>
          <p className="section-title">Edit Classification</p>
          <p style={{ marginTop: 0, color: 'var(--muted)' }}>{selectedRow.normalized_key}</p>
          <div className="layout-grid">
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Category</span>
              <select className="search-input" value={categoryId} onChange={(event) => onCategoryChange(event.target.value)}>
                {taxonomyCategories.map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}{category.enabled ? '' : ' (disabled)'}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Subcategory</span>
              <select className="search-input" value={subcategoryId} onChange={(event) => setSubcategoryId(event.target.value)}>
                {(selectedCategory?.subcategories ?? []).map((subcategory) => (
                  <option key={subcategory.id} value={subcategory.id}>
                    {subcategory.name}{subcategory.enabled ? '' : ' (disabled)'}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span style={{ display: 'block', marginBottom: '0.3rem' }}>Reason</span>
              <input className="search-input" value={reason} onChange={(event) => setReason(event.target.value)} />
            </label>
          </div>
          {taxonomyError ? (
            <p style={{ marginTop: '0.75rem', marginBottom: 0, color: '#ffcf7f' }}>
              Taxonomy unavailable: {taxonomyError}
            </p>
          ) : null}
          <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
            <button className="cta-button" disabled={!canCallApi || !canEdit || busyKey === selectedRow.normalized_key} onClick={saveClassification}>
              {busyKey === selectedRow.normalized_key ? 'Saving...' : 'Save'}
            </button>
            <button
              className="cta-button"
              style={{ background: 'linear-gradient(120deg,#f8d5d5,#cd7c7c)', color: '#25090c' }}
              disabled={!canCallApi || busyKey === selectedRow.normalized_key}
              onClick={() => removeClassification(selectedRow.normalized_key)}
            >
              {busyKey === selectedRow.normalized_key ? 'Removing...' : 'Remove Domain'}
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
          <div className="table-wrapper" role="region" tabIndex={0} aria-label="Classifications table">
            <table>
              <thead>
                <tr>
                  <th>Key</th>
                  <th>State</th>
                  <th>Category</th>
                  <th>Subcategory</th>
                  <th>Effective Action</th>
                  <th>Recorded Action</th>
                  <th>Updated</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {data.length === 0 ? (
                  <tr>
                    <td colSpan={8} style={{ textAlign: 'center', color: '#7f8fb2' }}>
                      No matching sites.
                    </td>
                  </tr>
                ) : (
                  data.map((item) => (
                    <tr key={item.normalized_key}>
                      <td>{item.normalized_key}</td>
                      <td>{item.state}</td>
                      <td>{item.primary_category ?? '—'}</td>
                      <td>{item.subcategory ?? '—'}</td>
                      <td>
                        {item.effective_action ?? '—'}
                        {item.effective_decision_source ? ` (${item.effective_decision_source})` : ''}
                      </td>
                      <td>{item.recommended_action ?? '—'}</td>
                      <td>{item.updated_at}</td>
                      <td style={{ textAlign: 'right' }}>
                        <button
                          className="cta-button"
                          style={{ padding: '0.4rem 0.8rem', fontSize: '0.75rem' }}
                          onClick={() => openEditor(item.normalized_key)}
                        >
                          Edit
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
