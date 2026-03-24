import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import { ReactElement } from 'react';
import AppLayout from './layout/AppLayout';
import { AuthProvider, Role, useAuth } from './context/AuthContext';
import { DashboardPage } from './pages/DashboardPage';
import { InvestigationsPage } from './pages/InvestigationsPage';
import { PoliciesPage } from './pages/PoliciesPage';
import { PolicyCreatePage } from './pages/PolicyCreatePage';
import { PolicyDetailPage } from './pages/PolicyDetailPage';
import { ReviewQueuePage } from './pages/ReviewQueuePage';
import { OverridesPage } from './pages/OverridesPage';
import { TaxonomyPage } from './pages/TaxonomyPage';
import { ReportsPage } from './pages/ReportsPage';
import { SettingsRbacPage } from './pages/SettingsRbacPage';
import { LoginPage } from './pages/LoginPage';
import { PendingClassificationsPage } from './pages/PendingClassificationsPage';

const guard = {
  viewer: ['policy-viewer'] as Role[],
  viewEdit: ['policy-viewer', 'policy-editor', 'policy-admin'] as Role[],
  editOnly: ['policy-editor', 'policy-admin'] as Role[],
  review: ['review-approver', 'policy-admin'] as Role[],
  reports: ['auditor', 'policy-admin', 'policy-viewer'] as Role[],
  admin: ['policy-admin'] as Role[],
};

const App = () => {
  return (
    <AuthProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route element={<ProtectedRoute><AppLayout /></ProtectedRoute>}>
            <Route path="/" element={<Navigate to="/dashboard" replace />} />
            <Route path="/dashboard" element={<ProtectedRoute><DashboardPage /></ProtectedRoute>} />
            <Route path="/investigations" element={<ProtectedRoute roles={guard.viewer}><InvestigationsPage /></ProtectedRoute>} />
            <Route path="/policies" element={<ProtectedRoute roles={guard.viewEdit}><PoliciesPage /></ProtectedRoute>} />
            <Route path="/policies/new" element={<ProtectedRoute roles={guard.editOnly}><PolicyCreatePage /></ProtectedRoute>} />
            <Route path="/policies/:policyId" element={<ProtectedRoute roles={guard.viewEdit}><PolicyDetailPage /></ProtectedRoute>} />
            <Route path="/review-queue" element={<ProtectedRoute roles={guard.review}><ReviewQueuePage /></ProtectedRoute>} />
            <Route path="/overrides" element={<ProtectedRoute roles={guard.editOnly}><OverridesPage /></ProtectedRoute>} />
            <Route path="/classifications/pending" element={<ProtectedRoute roles={guard.viewEdit}><PendingClassificationsPage /></ProtectedRoute>} />
            <Route path="/taxonomy" element={<ProtectedRoute roles={guard.editOnly}><TaxonomyPage /></ProtectedRoute>} />
            <Route path="/reports" element={<ProtectedRoute roles={guard.reports}><ReportsPage /></ProtectedRoute>} />
            <Route path="/settings/rbac" element={<ProtectedRoute roles={guard.admin}><SettingsRbacPage /></ProtectedRoute>} />
          </Route>
          <Route path="*" element={<Navigate to="/dashboard" replace />} />
        </Routes>
      </BrowserRouter>
    </AuthProvider>
  );
};

type GuardProps = {
  children: ReactElement;
  roles?: Role[];
};

export const ProtectedRoute = ({ children, roles }: GuardProps) => {
  const { user, hasAnyRole } = useAuth();
  if (!user) {
    return <Navigate to="/login" replace />;
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
