import { useEffect, useMemo, useState } from 'react';
import { useTaxonomyActions, type ActivationUpdatePayload } from '../hooks/useTaxonomyActions';
import { useTaxonomyData, type TaxonomyCategoryRow } from '../hooks/useTaxonomyData';

const cloneCategories = (categories: TaxonomyCategoryRow[]): TaxonomyCategoryRow[] =>
  categories.map((category) => ({
    ...category,
    subcategories: category.subcategories.map((sub) => ({ ...sub })),
  }));

const formatTimestamp = (value?: string) => {
  if (!value) return '—';
  try {
    return new Date(value).toLocaleString();
  } catch {
    return value;
  }
};

export const TaxonomyPage = () => {
  const { data, loading, error, isMock, refresh, canCallApi } = useTaxonomyData();
  const { saveActivation, busy, error: actionError } = useTaxonomyActions();
  const [localCategories, setLocalCategories] = useState<TaxonomyCategoryRow[]>([]);
  const [message, setMessage] = useState<string | undefined>();

  useEffect(() => {
    setLocalCategories(cloneCategories(data.categories));
  }, [data]);

  const isDirty = useMemo(() => {
    if (!data || localCategories.length === 0) return false;
    return localCategories.some((category, idx) => {
      const snapshot = data.categories[idx];
      if (!snapshot) return true;
      if (category.enabled !== snapshot.enabled) return true;
      return category.subcategories.some((sub, subIdx) => {
        const original = snapshot.subcategories[subIdx];
        return !original || sub.enabled !== original.enabled;
      });
    });
  }, [data, localCategories]);

  const canEdit = canCallApi && !isMock;

  const handleCategoryToggle = (categoryId: string, disabled: boolean) => {
    const enabled = !disabled;
    setLocalCategories((prev) =>
      prev.map((category) => {
        if (category.id !== categoryId || category.locked) return category;
        const nextSubcategories = category.subcategories.map((sub) => ({
          ...sub,
          enabled,
        }));
        return { ...category, enabled, subcategories: nextSubcategories };
      }),
    );
  };

  const handleSubcategoryToggle = (categoryId: string, subId: string, disabled: boolean) => {
    const enabled = !disabled;
    setLocalCategories((prev) =>
      prev.map((category) => {
        if (category.id !== categoryId || !category.enabled || category.locked) return category;
        const subcategories = category.subcategories.map((sub) => {
          if (sub.id !== subId || sub.locked) return sub;
          return { ...sub, enabled };
        });
        return { ...category, subcategories };
      }),
    );
  };

  const resetLocal = () => {
    setLocalCategories(cloneCategories(data.categories));
    setMessage(undefined);
  };

  const buildPayload = (version: string): ActivationUpdatePayload => ({
    version,
    categories: localCategories.map((category) => ({
      id: category.id,
      enabled: category.locked ? true : category.enabled,
      subcategories: category.subcategories.map((sub) => ({
        id: sub.id,
        enabled: sub.locked
          ? true
          : category.enabled
            ? sub.enabled
            : false,
      })),
    })),
  });

  const handleSave = async () => {
    if (!canEdit || !data || !isDirty) return;
    setMessage(undefined);
    try {
      await saveActivation(buildPayload(data.version));
      setMessage('Activation profile saved');
      await refresh();
    } catch {
      /* errors surface via hook */
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Taxonomy</p>
          <h2 style={{ margin: 0 }}>Canonical activation profile</h2>
        </div>
        <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
          <button
            className="cta-button"
            style={{ opacity: isDirty ? 1 : 0.7 }}
            disabled={!canEdit || busy || !isDirty}
            onClick={handleSave}
          >
            {busy ? 'Saving...' : 'Save Changes'}
          </button>
          <button
            className="cta-button"
            style={{ background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
            disabled={busy || !isDirty}
            onClick={resetLocal}
          >
            Reset
          </button>
          <button className="cta-button" onClick={refresh} disabled={loading}>
            Refresh
          </button>
        </div>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load taxonomy: {error}</p>
        </div>
      ) : null}

      {actionError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Save failed: {actionError}</p>
        </div>
      ) : null}

      {message ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(158, 247, 235, 0.4)' }}>
          <p style={{ margin: 0, color: '#9ef7eb' }}>{message}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: '#fdd744', marginTop: '0.5rem' }}>
          Mock stream (Admin API offline)
        </p>
      ) : null}

      <div className="glass-panel" style={{ marginTop: '1rem', background: 'rgba(108,140,255,0.08)' }}>
        <p style={{ margin: '0 0 0.3rem' }}>
          Taxonomy structure is locked to the canonical file. Checked boxes mean traffic is disabled/blocked; unchecked
          boxes mean the category/subcategory is allowed. Unknown / Unclassified traffic can now be disabled the same way.
          Re-enabling a category resets all of its topics to allowed so you can then disable specific subcategories.
        </p>
        <div style={{ display: 'flex', gap: '1.5rem', flexWrap: 'wrap', marginTop: '0.4rem' }}>
          <div>
            <p className="section-title" style={{ marginBottom: '0.2rem' }}>
              Version
            </p>
            <span>{data.version}</span>
          </div>
          <div>
            <p className="section-title" style={{ marginBottom: '0.2rem' }}>
              Updated at
            </p>
            <span>{formatTimestamp(data.updatedAt)}</span>
          </div>
          <div>
            <p className="section-title" style={{ marginBottom: '0.2rem' }}>
              Updated by
            </p>
            <span>{data.updatedBy ?? '—'}</span>
          </div>
        </div>
      </div>

      <div className="layout-grid" style={{ marginTop: '1rem' }}>
        {loading ? (
          <div className="glass-panel">
            {Array.from({ length: 4 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : null}

        {localCategories.map((category) => (
          <div key={category.id} className="glass-panel" style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '0.75rem' }}>
              <label style={{ display: 'flex', alignItems: 'center', gap: '0.6rem', fontWeight: 600 }}>
                <input
                  type="checkbox"
                  checked={category.enabled ? false : true}
                  disabled={!canEdit || busy || category.locked}
                  onChange={(event) => handleCategoryToggle(category.id, event.target.checked)}
                />
                {category.name}
              </label>
              {category.locked ? <span className="chip chip--teal">Locked</span> : null}
            </div>
            <div style={{ borderTop: '1px solid rgba(255,255,255,0.05)', paddingTop: '0.5rem' }}>
              <p className="section-title" style={{ marginBottom: '0.4rem' }}>Subcategories</p>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '0.4rem' }}>
                {category.subcategories.map((sub) => (
                  <label
                    key={sub.id}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: '0.75rem',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: '0.6rem' }}>
                      <input
                        type="checkbox"
                        checked={category.enabled ? (!sub.enabled) : true}
                        disabled={!category.enabled || !canEdit || busy || sub.locked}
                        onChange={(event) => handleSubcategoryToggle(category.id, sub.id, event.target.checked)}
                      />
                      <span>{sub.name}</span>
                    </div>
                    {sub.locked ? <span className="chip chip--slate">Locked</span> : null}
                  </label>
                ))}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
