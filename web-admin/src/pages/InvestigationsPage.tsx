import { useMemo, useState } from 'react';
import { investigations } from '../data/mockData';

export const InvestigationsPage = () => {
  const [query, setQuery] = useState('');
  const filtered = useMemo(() => {
    if (!query.trim()) return investigations;
    return investigations.filter((item) =>
      item.key.toLowerCase().includes(query.trim().toLowerCase()),
    );
  }, [query]);

  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">Investigations</p>
          <h2 style={{ margin: 0 }}>Classification History & Cache</h2>
        </div>
        <button className="cta-button">Open Timeline</button>
      </div>

      <input
        className="search-input"
        placeholder="Search normalized key, domain, reviewer..."
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />

      <div className="glass-panel" style={{ marginTop: '1.5rem' }}>
        <div className="table-wrapper">
          <table>
            <thead>
              <tr>
                <th>Key</th>
                <th>Verdict</th>
                <th>Risk</th>
                <th>Last Seen</th>
                <th>Tags</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((item) => (
                <tr key={item.key}>
                  <td>{item.key}</td>
                  <td>
                    <span className={`chip chip--${item.verdict === 'Block' ? 'red' : 'amber'}`}>
                      {item.verdict}
                    </span>
                  </td>
                  <td>{item.risk}</td>
                  <td>{item.lastSeen}</td>
                  <td>
                    {item.tags.map((tag) => (
                      <span key={tag} className="chip" style={{ marginRight: '0.35rem' }}>
                        {tag}
                      </span>
                    ))}
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
