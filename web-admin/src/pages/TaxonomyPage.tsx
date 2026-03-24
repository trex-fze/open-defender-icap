import { FormEvent, useState } from 'react';
import { useTaxonomyActions } from '../hooks/useTaxonomyActions';
import { useTaxonomyData } from '../hooks/useTaxonomyData';

const ACTION_OPTIONS = ['allow', 'block', 'warn', 'monitor', 'review', 'require-approval'];

export const TaxonomyPage = () => {
  const { data, loading, error, isMock, refresh, canCallApi } = useTaxonomyData();
  const {
    createCategory,
    updateCategory,
    deleteCategory,
    createSubcategory,
    updateSubcategory,
    deleteSubcategory,
    busy,
    error: actionError,
  } = useTaxonomyActions();

  const [selectedCategoryId, setSelectedCategoryId] = useState('');
  const [categoryName, setCategoryName] = useState('');
  const [categoryAction, setCategoryAction] = useState('warn');
  const [editingCategoryId, setEditingCategoryId] = useState<string | undefined>();

  const [subcategoryName, setSubcategoryName] = useState('');
  const [subcategoryAction, setSubcategoryAction] = useState('warn');
  const [editingSubcategoryId, setEditingSubcategoryId] = useState<string | undefined>();
  const [message, setMessage] = useState<string | undefined>();

  const submitCategory = async (event: FormEvent) => {
    event.preventDefault();
    setMessage(undefined);
    if (!categoryName.trim()) return;

    try {
      if (editingCategoryId) {
        await updateCategory(editingCategoryId, {
          name: categoryName,
          defaultAction: categoryAction,
        });
        setMessage('Category updated');
      } else {
        await createCategory({
          name: categoryName,
          defaultAction: categoryAction,
        });
        setMessage('Category created');
      }
      setEditingCategoryId(undefined);
      setCategoryName('');
      setCategoryAction('warn');
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  const submitSubcategory = async (event: FormEvent) => {
    event.preventDefault();
    setMessage(undefined);
    if (!selectedCategoryId || !subcategoryName.trim()) return;

    try {
      if (editingSubcategoryId) {
        await updateSubcategory(editingSubcategoryId, {
          categoryId: selectedCategoryId,
          name: subcategoryName,
          defaultAction: subcategoryAction,
        });
        setMessage('Subcategory updated');
      } else {
        await createSubcategory({
          categoryId: selectedCategoryId,
          name: subcategoryName,
          defaultAction: subcategoryAction,
        });
        setMessage('Subcategory created');
      }
      setEditingSubcategoryId(undefined);
      setSubcategoryName('');
      setSubcategoryAction('warn');
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  const beginEditCategory = (id: string, name: string, defaultAction: string) => {
    setEditingCategoryId(id);
    setCategoryName(name);
    setCategoryAction(defaultAction);
  };

  const beginEditSubcategory = (
    id: string,
    categoryId: string,
    name: string,
    defaultAction: string,
  ) => {
    setEditingSubcategoryId(id);
    setSelectedCategoryId(categoryId);
    setSubcategoryName(name);
    setSubcategoryAction(defaultAction);
  };

  const removeCategory = async (id: string) => {
    setMessage(undefined);
    try {
      await deleteCategory(id);
      setMessage('Category deleted');
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  const removeSubcategory = async (id: string) => {
    setMessage(undefined);
    try {
      await deleteSubcategory(id);
      setMessage('Subcategory deleted');
      await refresh();
    } catch {
      setMessage(undefined);
    }
  };

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Taxonomy</p>
          <h2 style={{ margin: 0 }}>Categories & subcategories</h2>
        </div>
        <button className="cta-button" onClick={refresh} disabled={loading}>
          Refresh
        </button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load taxonomy: {error}</p>
        </div>
      ) : null}

      {actionError ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Taxonomy change failed: {actionError}</p>
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

      <div className="layout-grid">
        <form className="glass-panel" onSubmit={submitCategory}>
          <p className="section-title">{editingCategoryId ? 'Edit Category' : 'Create Category'}</p>
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>Name</span>
            <input
              className="search-input"
              value={categoryName}
              onChange={(event) => setCategoryName(event.target.value)}
              placeholder="Social Media"
            />
          </label>
          <label>
            <span style={{ display: 'block', margin: '0.8rem 0 0.3rem' }}>Default action</span>
            <select
              className="search-input"
              value={categoryAction}
              onChange={(event) => setCategoryAction(event.target.value)}
            >
              {ACTION_OPTIONS.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>
          <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
            <button className="cta-button" disabled={!canCallApi || isMock || busy || !categoryName.trim()}>
              {busy ? 'Saving...' : editingCategoryId ? 'Update Category' : 'Create Category'}
            </button>
            <button
              type="button"
              className="cta-button"
              style={{ background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
              onClick={() => {
                setEditingCategoryId(undefined);
                setCategoryName('');
                setCategoryAction('warn');
              }}
            >
              Clear
            </button>
          </div>
        </form>

        <form className="glass-panel" onSubmit={submitSubcategory}>
          <p className="section-title">{editingSubcategoryId ? 'Edit Subcategory' : 'Create Subcategory'}</p>
          <label>
            <span style={{ display: 'block', marginBottom: '0.3rem' }}>Category</span>
            <select
              className="search-input"
              value={selectedCategoryId}
              onChange={(event) => setSelectedCategoryId(event.target.value)}
            >
              <option value="">Select category</option>
              {data.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span style={{ display: 'block', margin: '0.8rem 0 0.3rem' }}>Name</span>
            <input
              className="search-input"
              value={subcategoryName}
              onChange={(event) => setSubcategoryName(event.target.value)}
              placeholder="Forums"
            />
          </label>
          <label>
            <span style={{ display: 'block', margin: '0.8rem 0 0.3rem' }}>Default action</span>
            <select
              className="search-input"
              value={subcategoryAction}
              onChange={(event) => setSubcategoryAction(event.target.value)}
            >
              {ACTION_OPTIONS.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>
          <div style={{ marginTop: '1rem', display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
            <button
              className="cta-button"
              disabled={!canCallApi || isMock || busy || !selectedCategoryId || !subcategoryName.trim()}
            >
              {busy ? 'Saving...' : editingSubcategoryId ? 'Update Subcategory' : 'Create Subcategory'}
            </button>
            <button
              type="button"
              className="cta-button"
              style={{ background: 'linear-gradient(120deg,#d6def6,#8ca0cb)', color: '#060b17' }}
              onClick={() => {
                setEditingSubcategoryId(undefined);
                setSelectedCategoryId('');
                setSubcategoryName('');
                setSubcategoryAction('warn');
              }}
            >
              Clear
            </button>
          </div>
        </form>
      </div>

      <div className="layout-grid">
        {loading ? (
          <div className="glass-panel">
            {Array.from({ length: 4 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : null}

        {data.map((cat) => (
          <div key={cat.id} className="glass-panel">
            <h3 style={{ marginTop: 0 }}>{cat.name}</h3>
            <p style={{ color: 'var(--muted)', marginTop: '-0.4rem' }}>
              Default action: {cat.defaultAction}
            </p>
            <div style={{ display: 'flex', gap: '0.45rem', flexWrap: 'wrap', marginBottom: '0.75rem' }}>
              <button
                className="cta-button"
                style={{ padding: '0.4rem 0.8rem', fontSize: '0.75rem' }}
                onClick={() => beginEditCategory(cat.id, cat.name, cat.defaultAction)}
              >
                Edit Category
              </button>
              <button
                className="cta-button"
                style={{
                  padding: '0.4rem 0.8rem',
                  fontSize: '0.75rem',
                  background: 'linear-gradient(120deg,#ff9b9b,#fdd744)',
                  color: '#060b17',
                }}
                onClick={() => removeCategory(cat.id)}
                disabled={busy || !canCallApi || isMock}
              >
                Delete Category
              </button>
            </div>
            <ul style={{ listStyle: 'none', padding: 0 }}>
              {cat.subcategories.map((sub) => (
                <li
                  key={sub.id}
                  style={{
                    padding: '0.4rem 0',
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                  }}
                >
                  <span>{sub.name}</span>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', flexWrap: 'wrap' }}>
                    <span className="chip chip--amber">{sub.defaultAction}</span>
                    <button
                      className="cta-button"
                      style={{ padding: '0.35rem 0.65rem', fontSize: '0.7rem' }}
                      onClick={() => beginEditSubcategory(sub.id, sub.categoryId, sub.name, sub.defaultAction)}
                    >
                      Edit
                    </button>
                    <button
                      className="cta-button"
                      style={{
                        padding: '0.35rem 0.65rem',
                        fontSize: '0.7rem',
                        background: 'linear-gradient(120deg,#ff9b9b,#fdd744)',
                        color: '#060b17',
                      }}
                      onClick={() => removeSubcategory(sub.id)}
                      disabled={busy || !canCallApi || isMock}
                    >
                      Delete
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </div>
  );
};
