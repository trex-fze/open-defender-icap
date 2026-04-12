import { BrowserRouter, Navigate, Route, Routes, useLocation } from 'react-router-dom';
import { ReactElement } from 'react';
import AppLayout from './layout/AppLayout';
import { AuthProvider, Role, useAuth } from './context/AuthContext';
import { DashboardPage } from './pages/DashboardPage';
import { InvestigationsPage } from './pages/InvestigationsPage';
import { PoliciesPage } from './pages/PoliciesPage';
import { PolicyCreatePage } from './pages/PolicyCreatePage';
import { PolicyDetailPage } from './pages/PolicyDetailPage';
import { OverridesPage } from './pages/OverridesPage';
import { TaxonomyPage } from './pages/TaxonomyPage';
import { SettingsIamPage } from './pages/SettingsIamPage';
import { SettingsClassificationsPage } from './pages/SettingsClassificationsPage';
import { ChangePasswordPage } from './pages/ChangePasswordPage';
import { LoginPage } from './pages/LoginPage';
import { PendingClassificationsPage } from './pages/PendingClassificationsPage';
import { ClassificationsPage } from './pages/ClassificationsPage';
import { DiagnosticsCachePage } from './pages/DiagnosticsCachePage';
import { DiagnosticsPageContentPage } from './pages/DiagnosticsPageContentPage';
import { ThemeProvider } from './context/ThemeContext';

const guard = {
  viewer: ['policy-viewer'] as Role[],
  viewEdit: ['policy-viewer', 'policy-editor', 'policy-admin'] as Role[],
  editOnly: ['policy-editor', 'policy-admin'] as Role[],
  admin: ['policy-admin'] as Role[],
};

const App = () => {
  return (
    <ThemeProvider>
      <AuthProvider>
        <BrowserRouter>
          <Routes>
            <Route path="/login" element={<LoginPage />} />
            <Route path="/auth/change-password" element={<ProtectedRoute><ChangePasswordPage /></ProtectedRoute>} />
            <Route element={<ProtectedRoute><AppLayout /></ProtectedRoute>}>
              <Route path="/" element={<Navigate to="/dashboard" replace />} />
              <Route path="/dashboard" element={<ProtectedRoute><DashboardPage /></ProtectedRoute>} />
              <Route path="/investigations" element={<ProtectedRoute roles={guard.viewer}><InvestigationsPage /></ProtectedRoute>} />
              <Route path="/policies" element={<ProtectedRoute roles={guard.viewEdit}><PoliciesPage /></ProtectedRoute>} />
              <Route path="/policies/new" element={<ProtectedRoute roles={guard.editOnly}><PolicyCreatePage /></ProtectedRoute>} />
              <Route path="/policies/:policyId" element={<ProtectedRoute roles={guard.viewEdit}><PolicyDetailPage /></ProtectedRoute>} />
              <Route path="/overrides" element={<ProtectedRoute roles={guard.editOnly}><OverridesPage /></ProtectedRoute>} />
              <Route path="/allow-deny" element={<Navigate to="/overrides" replace />} />
              <Route path="/classifications/pending" element={<ProtectedRoute roles={guard.viewEdit}><PendingClassificationsPage /></ProtectedRoute>} />
              <Route path="/classifications" element={<ProtectedRoute roles={guard.viewEdit}><ClassificationsPage /></ProtectedRoute>} />
              <Route path="/taxonomy" element={<ProtectedRoute roles={guard.editOnly}><TaxonomyPage /></ProtectedRoute>} />
              <Route path="/diagnostics/cache" element={<ProtectedRoute roles={guard.admin}><DiagnosticsCachePage /></ProtectedRoute>} />
              <Route path="/diagnostics/page-content" element={<ProtectedRoute roles={guard.viewEdit}><DiagnosticsPageContentPage /></ProtectedRoute>} />
              <Route path="/settings/iam/*" element={<ProtectedRoute roles={guard.admin}><SettingsIamPage /></ProtectedRoute>} />
              <Route path="/settings/classifications" element={<ProtectedRoute roles={guard.admin}><SettingsClassificationsPage /></ProtectedRoute>} />
              <Route path="/settings/rbac" element={<Navigate to="/settings/iam" replace />} />
            </Route>
            <Route path="*" element={<Navigate to="/dashboard" replace />} />
          </Routes>
        </BrowserRouter>
      </AuthProvider>
    </ThemeProvider>
  );
};

type GuardProps = {
  children: ReactElement;
  roles?: Role[];
};

export const ProtectedRoute = ({ children, roles }: GuardProps) => {
  const { user, tokens, hasAnyRole } = useAuth();
  const location = useLocation();
  if (!user || !tokens?.accessToken) {
    return <Navigate to="/login" replace />;
  }
  if (user.mustChangePassword && location.pathname !== '/auth/change-password') {
    return <Navigate to="/auth/change-password" replace />;
  }
  if (!user.mustChangePassword && location.pathname === '/auth/change-password') {
    return <Navigate to="/dashboard" replace />;
  }
  if (roles && !hasAnyRole(roles as any)) {
    return (
      <div className="glass-panel">
        <h2>Insufficient permissions</h2>
        <p style={{ color: 'var(--muted)' }}>You need one of the following roles: {roles.join(', ')}</p>
      </div>
    );
  }
  return children;
};

export default App;
