import { useOverridesData } from '../hooks/useOverridesData';

export const OverridesPage = () => {
  const { data, loading, error, isMock } = useOverridesData();
  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Overrides</p>
          <h2 style={{ margin: 0 }}>Manual policy exceptions</h2>
        </div>
        <button className="cta-button">Import CSV</button>
      </div>

      {error ? (
        <div className="glass-panel" style={{ borderColor: 'rgba(255, 122, 122, 0.4)' }}>
          <p style={{ margin: 0, color: '#ff9b9b' }}>Failed to load overrides: {error}</p>
        </div>
      ) : null}

      {isMock ? (
        <p className="section-title" style={{ color: '#fdd744', marginTop: '0.5rem' }}>
          Mock stream (Admin API offline)
        </p>
      ) : null}

      <div className="glass-panel">
        {loading ? (
          <div>
            {Array.from({ length: 4 }).map((_, idx) => (
              <div key={idx} className="skeleton" style={{ marginBottom: '0.75rem' }}></div>
            ))}
          </div>
        ) : (
          <div className="table-wrapper">
            <table>
              <thead>
                <tr>
                  <th>Scope</th>
                  <th>Action</th>
                  <th>Expires</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {data.map((item) => (
                  <tr key={item.id}>
                    <td>{item.scope}</td>
                    <td>{item.action}</td>
                    <td>{item.expires}</td>
                    <td>
                      <span className={`chip chip--${item.status === 'active' ? 'green' : 'amber'}`}>
                        {item.status}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
