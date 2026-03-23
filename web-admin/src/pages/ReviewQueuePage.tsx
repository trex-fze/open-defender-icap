import { reviewQueue } from '../data/mockData';

export const ReviewQueuePage = () => {
  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Review Queue</p>
          <h2 style={{ margin: 0 }}>Human-in-the-loop decisions</h2>
        </div>
        <button className="cta-button">Bulk Resolve</button>
      </div>

      <div className="glass-panel">
        <div className="table-wrapper">
          <table>
            <thead>
              <tr>
                <th>Key</th>
                <th>Status</th>
                <th>Risk</th>
                <th>SLA</th>
              </tr>
            </thead>
            <tbody>
              {reviewQueue.map((item) => (
                <tr key={item.id}>
                  <td>{item.key}</td>
                  <td>{item.status}</td>
                  <td>
                    <span className={`chip chip--${item.risk === 'critical' ? 'red' : 'amber'}`}>
                      {item.risk}
                    </span>
                  </td>
                  <td>{item.sla}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
