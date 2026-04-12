import { NavLink, Outlet, useLocation } from 'react-router-dom';
import { ReactNode, useEffect, useState } from 'react';
import { Role, useAuth } from '../context/AuthContext';

const MOBILE_BREAKPOINT = 960;
const SIDEBAR_COLLAPSE_KEY = 'od.sidebar.collapsed';

type NavItem = {
  label: string;
  path: string;
  glyph: string;
  exact?: boolean;
  roles?: Role[];
};

const navItems: NavItem[] = [
  { label: 'Dashboard', path: '/dashboard', glyph: 'DB' },
  { label: 'Investigations', path: '/investigations', glyph: 'IN', roles: ['policy-viewer'] as Role[] },
  { label: 'Policies', path: '/policies', glyph: 'PL', roles: ['policy-editor', 'policy-admin', 'policy-viewer'] as Role[] },
  {
    label: 'Pending Sites',
    path: '/classifications/pending',
    glyph: 'PS',
    roles: ['policy-viewer', 'policy-editor', 'policy-admin'] as Role[],
  },
  {
    label: 'Classifications',
    path: '/classifications',
    glyph: 'CL',
    exact: true,
    roles: ['policy-viewer', 'policy-editor', 'policy-admin'] as Role[],
  },
  { label: 'Allow / Deny', path: '/overrides', glyph: 'AD', roles: ['policy-editor', 'policy-admin'] as Role[] },
  { label: 'Taxonomy', path: '/taxonomy', glyph: 'TX', roles: ['policy-editor', 'policy-admin'] as Role[] },
  { label: 'Page Content', path: '/diagnostics/page-content', glyph: 'PC', roles: ['policy-editor', 'policy-admin', 'policy-viewer'] as Role[] },
  { label: 'Cache', path: '/diagnostics/cache', glyph: 'CH', roles: ['policy-admin'] as Role[] },
  { label: 'Settings', path: '/settings/rbac', glyph: 'ST', roles: ['policy-admin'] as Role[] },
];

const isMobileWidth = () => (typeof window !== 'undefined' ? window.innerWidth <= MOBILE_BREAKPOINT : false);
const readCollapsedPreference = () => {
  if (typeof window === 'undefined') {
    return false;
  }
  return window.localStorage.getItem(SIDEBAR_COLLAPSE_KEY) === 'true';
};

const AppLayout = () => {
  const { user, hasAnyRole, logout } = useAuth();
  const location = useLocation();
  const [isDesktopCollapsed, setIsDesktopCollapsed] = useState<boolean>(readCollapsedPreference);
  const [isMobile, setIsMobile] = useState<boolean>(isMobileWidth);
  const [isMobileOpen, setIsMobileOpen] = useState(false);
  const isSidebarCollapsed = !isMobile && isDesktopCollapsed;

  useEffect(() => {
    const onResize = () => {
      const mobile = window.innerWidth <= MOBILE_BREAKPOINT;
      setIsMobile(mobile);
      if (!mobile) setIsMobileOpen(false);
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  useEffect(() => {
    setIsMobileOpen(false);
  }, [location.pathname]);

  useEffect(() => {
    if (!isMobileOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsMobileOpen(false);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isMobileOpen]);

  useEffect(() => {
    if (typeof document === 'undefined') return;
    if (isMobile && isMobileOpen) {
      document.body.style.overflow = 'hidden';
      return;
    }
    document.body.style.overflow = '';
  }, [isMobile, isMobileOpen]);

  const toggleDesktopSidebar = () => {
    setIsDesktopCollapsed((prev) => {
      const next = !prev;
      if (typeof window !== 'undefined') {
        window.localStorage.setItem(SIDEBAR_COLLAPSE_KEY, next ? 'true' : 'false');
      }
      return next;
    });
  };

  const toggleMobileSidebar = () => setIsMobileOpen((prev) => !prev);
  const closeMobileSidebar = () => setIsMobileOpen(false);

  const mobileToggleLabel = isMobileOpen ? 'Hide menu' : 'Show menu';
  const desktopToggleLabel = isSidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar';
  const userInitial = (user?.name?.trim()?.charAt(0) || 'G').toUpperCase();

  return (
    <div
      className={`app-shell${isSidebarCollapsed ? ' sidebar-collapsed' : ''}${isMobile ? ' sidebar-mobile' : ''}${isMobileOpen ? ' sidebar-open-mobile' : ''}`}
    >
      <button
        type="button"
        className="sidebar-toggle sidebar-toggle-mobile"
        aria-controls="primary-sidebar"
        aria-expanded={isMobileOpen}
        aria-label={mobileToggleLabel}
        onClick={toggleMobileSidebar}
      >
        {mobileToggleLabel}
      </button>
      <aside className="sidebar" id="primary-sidebar">
        <button
          type="button"
          className="sidebar-toggle sidebar-toggle-desktop"
          aria-controls="primary-sidebar"
          aria-expanded={!isSidebarCollapsed}
          aria-label={desktopToggleLabel}
          onClick={toggleDesktopSidebar}
        >
          {isSidebarCollapsed ? '>>' : '<<'}
        </button>
        <div className="sidebar-brand-row">
          <img className="sidebar-brand-logo" src="/brand/logo.png" alt="Open Defender ICAP" />
        </div>
        <nav className="sidebar-nav" aria-label="Primary">
          {navItems
            .filter((item) => !item.roles || hasAnyRole(item.roles))
            .map((item) => (
              <NavLink
                key={item.path}
                to={item.path}
                end={item.exact}
                title={item.label}
                aria-label={item.label}
                className={({ isActive }) => `nav-link${isActive ? ' active' : ''}`}
                onClick={() => {
                  if (isMobile) {
                    closeMobileSidebar();
                  }
                }}
              >
                <span className="nav-link__icon" aria-hidden="true">{item.glyph}</span>
                <span className="nav-link__label">{item.label}</span>
              </NavLink>
            ))}
        </nav>
        <UserBadge>
          {isSidebarCollapsed ? (
            <div className="user-collapsed-controls">
              <div className="user-chip" title={user?.name ?? 'Guest'}>{userInitial}</div>
              {user ? (
                <button type="button" className="sidebar-logout sidebar-logout-compact" aria-label="Logout" title="Logout" onClick={logout}>
                  LO
                </button>
              ) : null}
            </div>
          ) : (
            <>
              <div className="user-meta">
                <strong>{user?.name ?? 'Guest'}</strong>
                <p style={{ margin: 0, color: '#7f8fb2', fontSize: '0.85rem' }}>{user?.email ?? 'not signed in'}</p>
              </div>
              {user ? (
                <button type="button" className="sidebar-logout" onClick={logout}>Logout</button>
              ) : null}
            </>
          )}
        </UserBadge>
      </aside>
      <button
        type="button"
        className="sidebar-backdrop"
        aria-label="Close menu"
        onClick={closeMobileSidebar}
      />
      <section className="main-panel">
        <Outlet />
      </section>
    </div>
  );
};

const UserBadge = ({ children }: { children: ReactNode }) => (
  <div className="user-badge">
    {children}
  </div>
);

export default AppLayout;
