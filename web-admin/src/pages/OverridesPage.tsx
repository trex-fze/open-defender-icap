import { overrides } from '../data/mockData';

export const OverridesPage = () => {
  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Overrides</p>
          <h2 style={{ margin: 0 }}>Manual policy exceptions</h2>
        </div>
        <button className="cta-button">Import CSV</button>
      </div>

      <div className="glass-panel">
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
              {overrides.map((item) => (
                <tr key={item.id}>
                  <td>{item.scope}</td>
                  <td>{item.action}</td>
                  <td>{item.expires}</td>
                  <td>
                    <span className="chip chip--green">{item.status}</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
