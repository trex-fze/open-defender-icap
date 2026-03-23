import { taxonomy } from '../data/mockData';

export const TaxonomyPage = () => {
  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Taxonomy</p>
          <h2 style={{ margin: 0 }}>Categories & subcategories</h2>
        </div>
        <button className="cta-button">New Category</button>
      </div>

      <div className="layout-grid">
        {taxonomy.categories.map((cat) => (
          <div key={cat.id} className="glass-panel">
            <h3 style={{ marginTop: 0 }}>{cat.name}</h3>
            <p style={{ color: 'var(--muted)', marginTop: '-0.4rem' }}>Default action: {cat.defaultAction}</p>
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
                  <span className="chip chip--amber">{sub.defaultAction}</span>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </div>
  );
};
