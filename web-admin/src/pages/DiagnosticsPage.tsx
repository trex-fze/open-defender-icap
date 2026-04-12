import { NavLink, Outlet } from 'react-router-dom';
import { Role, useAuth } from '../context/AuthContext';

const tabs: Array<{ label: string; path: string; roles: Role[] }> = [
  { label: 'Cache', path: 'cache', roles: ['policy-admin'] },
  { label: 'Page Content', path: 'page-content', roles: ['policy-viewer', 'policy-editor', 'policy-admin'] },
];

export const DiagnosticsPage = () => {
  const { hasAnyRole } = useAuth();
  const visibleTabs = tabs.filter((tab) => hasAnyRole(tab.roles));

  return (
    <div>
      <div className="page-header" style={{ marginBottom: '1.2rem' }}>
        <div>
          <p className="section-title">Diagnostics</p>
          <h2 style={{ margin: 0 }}>Cache and Page Content</h2>
          <p style={{ color: 'var(--muted)', marginTop: '0.35rem' }}>
            Inspect lookup paths and cached content artifacts from one operator workspace.
          </p>
        </div>
      </div>

      <div className="glass-panel diagnostics-shell">
        <nav className="diagnostics-tabs" aria-label="Diagnostics sections">
          {visibleTabs.map((tab) => (
            <NavLink
              key={tab.path}
              to={tab.path}
              end
              className={({ isActive }) => `diagnostics-tab${isActive ? ' diagnostics-tab--active' : ''}`}
            >
              {tab.label}
            </NavLink>
          ))}
        </nav>
        <div className="diagnostics-shell-body">
          <Outlet />
        </div>
      </div>
    </div>
  );
};
