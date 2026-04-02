import { NavLink, Outlet } from 'react-router-dom';
import { ReactNode } from 'react';
import { Role, useAuth } from '../context/AuthContext';

type NavItem = {
  label: string;
  path: string;
  roles?: Role[];
};

const navItems: NavItem[] = [
  { label: 'Dashboard', path: '/dashboard' },
  { label: 'Investigations', path: '/investigations', roles: ['policy-viewer'] as Role[] },
  { label: 'Policies', path: '/policies', roles: ['policy-editor', 'policy-admin', 'policy-viewer'] as Role[] },
  {
    label: 'Pending Sites',
    path: '/classifications/pending',
    roles: ['policy-viewer', 'policy-editor', 'policy-admin'] as Role[],
  },
  {
    label: 'Classifications',
    path: '/classifications',
    roles: ['policy-viewer', 'policy-editor', 'policy-admin'] as Role[],
  },
  { label: 'Allow / Deny', path: '/overrides', roles: ['policy-editor', 'policy-admin'] as Role[] },
  { label: 'Taxonomy', path: '/taxonomy', roles: ['policy-editor', 'policy-admin'] as Role[] },
  { label: 'Reports', path: '/reports', roles: ['auditor', 'policy-admin', 'policy-viewer'] as Role[] },
  { label: 'Page Content', path: '/diagnostics/page-content', roles: ['policy-editor', 'policy-admin', 'policy-viewer'] as Role[] },
  { label: 'Cache', path: '/diagnostics/cache', roles: ['policy-admin'] as Role[] },
  { label: 'Settings', path: '/settings/rbac', roles: ['policy-admin'] as Role[] },
];

const AppLayout = () => {
  const { user, hasAnyRole, logout } = useAuth();

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div>
          <h1>Open Defender</h1>
          <p style={{ marginTop: '-0.5rem', color: '#6c7da5', fontSize: '0.9rem' }}>
            Async Classification Control
          </p>
        </div>
        <nav style={{ display: 'flex', flexDirection: 'column', gap: '0.4rem', flex: 1 }}>
          {navItems
            .filter((item) => !item.roles || hasAnyRole(item.roles))
            .map((item) => (
              <NavLink key={item.path} to={item.path} className={({ isActive }) => `nav-link${isActive ? ' active' : ''}`}>
                {item.label}
              </NavLink>
            ))}
        </nav>
        <UserBadge>
          <div>
            <strong>{user?.name ?? 'Guest'}</strong>
            <p style={{ margin: 0, color: '#7f8fb2', fontSize: '0.85rem' }}>{user?.email ?? 'not signed in'}</p>
          </div>
          {user ? (
            <button className="cta-button" style={{ marginTop: '0.75rem', fontSize: '0.75rem' }} onClick={logout}>
              Logout
            </button>
          ) : null}
        </UserBadge>
      </aside>
      <section className="main-panel">
        <Outlet />
      </section>
    </div>
  );
};

const UserBadge = ({ children }: { children: ReactNode }) => (
  <div
    style={{
      marginTop: 'auto',
      padding: '1rem',
      borderRadius: '1rem',
      background: 'rgba(255,255,255,0.05)',
      border: '1px solid rgba(255,255,255,0.1)',
    }}
  >
    {children}
  </div>
);

export default AppLayout;
