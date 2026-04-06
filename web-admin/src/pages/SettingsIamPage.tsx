import { FormEvent, useCallback, useEffect, useMemo, useState } from 'react';
import { NavLink, Navigate, Route, Routes } from 'react-router-dom';
import {
  adminDelete,
  adminGetJson,
  adminPostJson,
  type AdminApiContext,
} from '../api/adminClient';
import { PaginationControls } from '../components/PaginationControls';
import { useAdminApi } from '../hooks/useAdminApi';
import type {
  IamAuditRecord,
  IamGroupDetails,
  IamRoleRecord,
  IamUserDetails,
  ServiceAccountDetails,
  ServiceAccountWithToken,
} from '../types/iam';
import type { CursorMeta, CursorPaged } from '../types/pagination';

const tabs = [
  { path: 'users', label: 'Users' },
  { path: 'groups', label: 'Groups' },
  { path: 'roles', label: 'Roles' },
  { path: 'service-accounts', label: 'Service Accounts' },
  { path: 'audit', label: 'Audit Log' },
];

export const SettingsIamPage = () => {
  return (
    <div className="settings-shell">
      <div className="page-header" style={{ marginBottom: '1.5rem' }}>
        <div>
          <p className="section-title">Identity & Access</p>
          <h2 style={{ margin: 0 }}>IAM Workspace</h2>
          <p style={{ color: 'var(--muted)', marginTop: '0.35rem' }}>
            Manage users, groups, roles, service accounts, and audit evidence from a single canvas.
          </p>
        </div>
      </div>
      <div className="glass-panel" style={{ paddingBottom: 0 }}>
        <nav className="iam-tabs" style={{ marginBottom: '1rem' }}>
          <NavLink to="/settings/iam" className="iam-tab iam-tab--active">
            IAM Workspace
          </NavLink>
          <NavLink
            to="/settings/classifications"
            className={({ isActive }) => `iam-tab ${isActive ? 'iam-tab--active' : ''}`}
          >
            Classifications Exchange
          </NavLink>
        </nav>
        <nav className="iam-tabs">
          {tabs.map((tab) => (
            <NavLink
              key={tab.path}
              to={tab.path}
              className={({ isActive }) =>
                `iam-tab ${isActive ? 'iam-tab--active' : ''}`
              }
            >
              {tab.label}
            </NavLink>
          ))}
        </nav>
        <div className="iam-panel">
          <Routes>
            <Route path="users" element={<IamUsersPanel />} />
            <Route path="groups" element={<IamGroupsPanel />} />
            <Route path="roles" element={<IamRolesPanel />} />
            <Route path="service-accounts" element={<IamServiceAccountsPanel />} />
            <Route path="audit" element={<IamAuditPanel />} />
            <Route path="*" element={<Navigate to="users" replace />} />
          </Routes>
        </div>
      </div>
    </div>
  );
};

const useIamRoles = () => {
  const api = useAdminApi();
  const [roles, setRoles] = useState<IamRoleRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  const loadRoles = useCallback(async () => {
    if (!api.canCallApi) return;
    setLoading(true);
    setError(undefined);
    try {
      const data = await adminGetJson<IamRoleRecord[]>(api as AdminApiContext, '/api/v1/iam/roles');
      setRoles(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load role catalog');
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => {
    loadRoles();
  }, [loadRoles]);

  return { roles, loading, error, reload: loadRoles } as const;
};

const IamUsersPanel = () => {
  const api = useAdminApi();
  const { roles } = useIamRoles();
  const [authMode, setAuthMode] = useState<'local' | 'hybrid' | 'oidc'>('local');
  const [users, setUsers] = useState<IamUserDetails[]>([]);
  const [meta, setMeta] = useState<CursorMeta>({ limit: 50, has_more: false });
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [selectedRole, setSelectedRole] = useState<Record<string, string>>({});
  const [busyUser, setBusyUser] = useState<string>();
  const [form, setForm] = useState({
    username: '',
    email: '',
    display_name: '',
    password: '',
    must_change_password: true,
    subject: '',
    status: 'active',
  });
  const [lastUserToken, setLastUserToken] = useState<{ userId: string; username: string; token: string }>();

  const loadUsers = useCallback(async () => {
    if (!api.canCallApi) return;
    setLoading(true);
    setError(undefined);
    try {
      const body = await adminGetJson<CursorPaged<IamUserDetails>>(
        api as AdminApiContext,
        '/api/v1/iam/users',
        { limit: meta.limit, cursor },
      );
      setUsers(body.data);
      setMeta(body.meta);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load users');
    } finally {
      setLoading(false);
    }
  }, [api, cursor, meta.limit]);

  useEffect(() => {
    loadUsers();
  }, [loadUsers]);

  useEffect(() => {
    if (!api.canCallApi) return;
    adminGetJson<{ mode: 'local' | 'hybrid' | 'oidc' }>(
      api as AdminApiContext,
      '/api/v1/auth/mode',
    )
      .then((body) => setAuthMode(body.mode ?? 'local'))
      .catch(() => {
        setAuthMode('local');
      });
  }, [api]);

  const handleCreate = async (event: FormEvent) => {
    event.preventDefault();
    if (!form.username.trim()) {
      setError('Username is required');
      return;
    }
    try {
      setLastUserToken(undefined);
      await adminPostJson(api as AdminApiContext, '/api/v1/iam/users', {
        username: form.username.trim(),
        email: form.email.trim() || undefined,
        display_name: form.display_name.trim() || undefined,
        password: form.password.trim() || undefined,
        must_change_password: form.must_change_password,
        subject: form.subject.trim() || undefined,
        status: form.status,
      });
      setForm({
        username: '',
        email: '',
        display_name: '',
        password: '',
        must_change_password: true,
        subject: '',
        status: 'active',
      });
      loadUsers();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create user');
    }
  };

  const assignRole = async (userId: string) => {
    const role = selectedRole[userId];
    if (!role) return;
    setBusyUser(userId);
    try {
      await adminPostJson(api as AdminApiContext, `/api/v1/iam/users/${userId}/roles`, {
        role,
      });
      setSelectedRole((prev) => ({ ...prev, [userId]: '' }));
      loadUsers();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to assign role');
    } finally {
      setBusyUser(undefined);
    }
  };

  const revokeRole = async (userId: string, role: string) => {
    setBusyUser(userId);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/iam/users/${userId}/roles/${role}`);
      loadUsers();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to remove role');
    } finally {
      setBusyUser(undefined);
    }
  };

  const disableUser = async (userId: string) => {
    setBusyUser(userId);
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/iam/users/${userId}`);
      loadUsers();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to disable user');
    } finally {
      setBusyUser(undefined);
    }
  };

  const setUserPassword = async (userId: string) => {
    const nextPassword = window.prompt('Enter new password');
    if (!nextPassword) return;
    setBusyUser(userId);
    try {
      await adminPostJson(api as AdminApiContext, `/api/v1/iam/users/${userId}/set-password`, {
        password: nextPassword,
        must_change_password: true,
      });
      setError(undefined);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set user password');
    } finally {
      setBusyUser(undefined);
    }
  };

  const createUserApiKey = async (userId: string, username: string) => {
    const name = window.prompt('Token name', `${username}-token`);
    if (!name) return;
    setBusyUser(userId);
    try {
      const response = await adminPostJson<{ token: string } & Record<string, any>>(
        api as AdminApiContext,
        `/api/v1/iam/users/${userId}/tokens`,
        { name: name.trim() },
      );
      setLastUserToken({ userId, username, token: response.token });
      setError(undefined);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create user API key');
    } finally {
      setBusyUser(undefined);
    }
  };

  return (
    <section>
      <header className="iam-section-head">
        <div>
          <h3>Directory</h3>
          <p>Create operator accounts and align their role membership with policy guardrails.</p>
        </div>
      </header>
      <form className="iam-form" onSubmit={handleCreate}>
        <div className="iam-form-grid">
          <label>
            <span>Username</span>
            <input
              value={form.username}
              onChange={(e) => setForm((prev) => ({ ...prev, username: e.target.value }))}
              placeholder="casey"
              required
            />
          </label>
          <label>
            <span>Email (optional)</span>
            <input
              value={form.email}
              onChange={(e) => setForm((prev) => ({ ...prev, email: e.target.value }))}
              placeholder="casey@example.com"
            />
          </label>
          <label>
            <span>Display name</span>
            <input
              value={form.display_name}
              onChange={(e) => setForm((prev) => ({ ...prev, display_name: e.target.value }))}
              placeholder="Casey Lin"
            />
          </label>
          <label>
            <span>Initial password (optional)</span>
            <input
              value={form.password}
              onChange={(e) => setForm((prev) => ({ ...prev, password: e.target.value }))}
              type="password"
              placeholder="Set a temporary password"
            />
          </label>
          <label>
            <span>Require password change</span>
            <select
              value={form.must_change_password ? 'yes' : 'no'}
              onChange={(e) =>
                setForm((prev) => ({ ...prev, must_change_password: e.target.value === 'yes' }))
              }
            >
              <option value="yes">yes</option>
              <option value="no">no</option>
            </select>
          </label>
          {authMode === 'local' ? (
            <details>
              <summary style={{ cursor: 'pointer', color: 'var(--muted)' }}>
                Advanced: external identity mapping
              </summary>
              <label style={{ marginTop: '0.5rem', display: 'block' }}>
                <span>External IdP Subject (optional)</span>
                <input
                  value={form.subject}
                  onChange={(e) => setForm((prev) => ({ ...prev, subject: e.target.value }))}
                  placeholder="00u123..."
                />
              </label>
            </details>
          ) : (
            <label>
              <span>External IdP Subject (optional)</span>
              <input
                value={form.subject}
                onChange={(e) => setForm((prev) => ({ ...prev, subject: e.target.value }))}
                placeholder="00u123..."
              />
            </label>
          )}
          <label>
            <span>Status</span>
            <select
              value={form.status}
              onChange={(e) => setForm((prev) => ({ ...prev, status: e.target.value }))}
            >
              <option value="active">active</option>
              <option value="disabled">disabled</option>
            </select>
          </label>
        </div>
        <button className="cta-button" disabled={!api.canCallApi}>Create User</button>
      </form>
      {lastUserToken && (
        <div className="glass-panel" style={{ marginBottom: '1rem' }}>
          <p className="section-title">New API key for {lastUserToken.username}</p>
          <code className="token-display">{lastUserToken.token}</code>
          <p className="muted" style={{ marginTop: '0.35rem' }}>
            Copy now — this key is shown only once.
          </p>
        </div>
      )}
      {error && <div className="error-banner">{error}</div>}
      <PaginationControls
        limit={meta.limit}
        loading={loading}
        hasMore={Boolean(meta.next_cursor) && meta.has_more}
        canGoBack={cursorStack.length > 0}
        onPrev={() => {
          setCursorStack((prev) => {
            if (prev.length === 0) return prev;
            const next = [...prev];
            const previousCursor = next.pop();
            setCursor(previousCursor || undefined);
            return next;
          });
        }}
        onNext={() => {
          if (!meta.next_cursor) return;
          setCursorStack((prev) => [...prev, cursor ?? '']);
          setCursor(meta.next_cursor);
        }}
        onLimitChange={(nextLimit) => {
          setMeta((prev) => ({ ...prev, limit: nextLimit }));
          setCursor(undefined);
          setCursorStack([]);
        }}
      />
      <div className="table-wrapper" role="region" tabIndex={0} aria-label="Users table">
        {loading ? (
          <p className="muted">Loading users…</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>User</th>
                <th>Roles</th>
                <th>Groups</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {users.map((entry) => (
                <tr key={entry.user.id}>
                  <td>
                    <strong>{entry.user.display_name || entry.user.username || entry.user.email || 'Unnamed user'}</strong>
                    <div className="muted" style={{ fontSize: '0.85rem' }}>
                      {(entry.user.username || 'no-username')}
                      {entry.user.email ? ` · ${entry.user.email}` : ''} · {entry.user.status}
                    </div>
                  </td>
                  <td>
                    <div className="chip-row">
                      {entry.roles.map((role) => (
                        <span key={role} className="chip">
                          {role}
                          <button
                            type="button"
                            onClick={() => revokeRole(entry.user.id, role)}
                            disabled={busyUser === entry.user.id}
                            aria-label={`Remove ${role}`}
                          >
                            ×
                          </button>
                        </span>
                      ))}
                    </div>
                    <div className="inline-form">
                      <select
                        value={selectedRole[entry.user.id] || ''}
                        onChange={(e) =>
                          setSelectedRole((prev) => ({ ...prev, [entry.user.id]: e.target.value }))
                        }
                      >
                        <option value="">Select role</option>
                        {roles.map((role) => (
                          <option key={role.id} value={role.name}>
                            {role.name}
                          </option>
                        ))}
                      </select>
                      <button
                        type="button"
                        className="ghost-button"
                        onClick={() => assignRole(entry.user.id)}
                        disabled={busyUser === entry.user.id || !selectedRole[entry.user.id]}
                      >
                        Assign
                      </button>
                    </div>
                  </td>
                  <td>
                    <div className="chip-row">
                      {entry.groups.map((group) => (
                        <span key={group.id} className="chip subtle">
                          {group.name}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td>
                    <div className="button-stack" style={{ gap: '0.5rem' }}>
                      <button
                        className="ghost-button"
                        disabled={busyUser === entry.user.id}
                        onClick={() => setUserPassword(entry.user.id)}
                      >
                        Set Password
                      </button>
                      <button
                        className="ghost-button"
                        disabled={busyUser === entry.user.id}
                        onClick={() => createUserApiKey(entry.user.id, entry.user.username || entry.user.email || 'user')}
                      >
                        Create API Key
                      </button>
                    </div>
                    <div style={{ marginTop: '0.5rem' }}>
                    <button
                      className="ghost-button"
                      disabled={busyUser === entry.user.id}
                      onClick={() => disableUser(entry.user.id)}
                    >
                      Disable
                    </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
};

const IamGroupsPanel = () => {
  const api = useAdminApi();
  const { roles: roleCatalog } = useIamRoles();
  const [groups, setGroups] = useState<IamGroupDetails[]>([]);
  const [directory, setDirectory] = useState<IamUserDetails[]>([]);
  const [meta, setMeta] = useState<CursorMeta>({ limit: 50, has_more: false });
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [form, setForm] = useState({ name: '', description: '' });
  const [memberSelection, setMemberSelection] = useState<Record<string, string>>({});
  const [roleSelection, setRoleSelection] = useState<Record<string, string>>({});

  const loadGroups = useCallback(async () => {
    if (!api.canCallApi) return;
    setLoading(true);
    setError(undefined);
    try {
      const body = await adminGetJson<CursorPaged<IamGroupDetails>>(
        api as AdminApiContext,
        '/api/v1/iam/groups',
        { limit: meta.limit, cursor },
      );
      setGroups(body.data);
      setMeta(body.meta);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load groups');
    } finally {
      setLoading(false);
    }
  }, [api, cursor, meta.limit]);

  const loadDirectory = useCallback(async () => {
    if (!api.canCallApi) return;
    try {
      const body = await adminGetJson<CursorPaged<IamUserDetails>>(
        api as AdminApiContext,
        '/api/v1/iam/users',
        { limit: 200 },
      );
      setDirectory(body.data);
    } catch {
      // no-op
    }
  }, [api]);

  useEffect(() => {
    loadGroups();
    loadDirectory();
  }, [loadGroups, loadDirectory]);

  const createGroup = async (event: FormEvent) => {
    event.preventDefault();
    if (!form.name.trim()) {
      setError('Name is required');
      return;
    }
    try {
      await adminPostJson(api as AdminApiContext, '/api/v1/iam/groups', {
        name: form.name.trim(),
        description: form.description.trim() || undefined,
      });
      setForm({ name: '', description: '' });
      loadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create group');
    }
  };

  const addMember = async (groupId: string) => {
    const userId = memberSelection[groupId];
    if (!userId) return;
    try {
      await adminPostJson(api as AdminApiContext, `/api/v1/iam/groups/${groupId}/members`, {
        user_id: userId,
      });
      setMemberSelection((prev) => ({ ...prev, [groupId]: '' }));
      loadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to add member');
    }
  };

  const removeMember = async (groupId: string, userId: string) => {
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/iam/groups/${groupId}/members/${userId}`);
      loadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to remove member');
    }
  };

  const deleteGroup = async (groupId: string) => {
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/iam/groups/${groupId}`);
      loadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete group');
    }
  };

  const assignRole = async (groupId: string) => {
    const role = roleSelection[groupId];
    if (!role) return;
    try {
      await adminPostJson(api as AdminApiContext, `/api/v1/iam/groups/${groupId}/roles`, {
        role,
      });
      setRoleSelection((prev) => ({ ...prev, [groupId]: '' }));
      loadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to assign role');
    }
  };

  const revokeRole = async (groupId: string, role: string) => {
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/iam/groups/${groupId}/roles/${role}`);
      loadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to revoke role');
    }
  };

  const directoryOptions = useMemo(
    () =>
      directory.map((entry) => ({
        id: entry.user.id,
        label: entry.user.display_name || entry.user.username || entry.user.email || entry.user.id,
      })),
    [directory],
  );

  return (
    <section>
      <header className="iam-section-head">
        <div>
          <h3>Groups</h3>
          <p>Bundle users into reusable role grants for regional or functional access.</p>
        </div>
      </header>
      <form className="iam-form" onSubmit={createGroup}>
        <div className="iam-form-grid">
          <label>
            <span>Name</span>
            <input
              value={form.name}
              onChange={(e) => setForm((prev) => ({ ...prev, name: e.target.value }))}
              placeholder="APAC Policy Editors"
              required
            />
          </label>
          <label>
            <span>Description</span>
            <input
              value={form.description}
              onChange={(e) => setForm((prev) => ({ ...prev, description: e.target.value }))}
              placeholder="Regional override maintainers"
            />
          </label>
        </div>
        <button className="cta-button" disabled={!api.canCallApi}>Create Group</button>
      </form>
      {error && <div className="error-banner">{error}</div>}
      <PaginationControls
        limit={meta.limit}
        loading={loading}
        hasMore={Boolean(meta.next_cursor) && meta.has_more}
        canGoBack={cursorStack.length > 0}
        onPrev={() => {
          setCursorStack((prev) => {
            if (prev.length === 0) return prev;
            const next = [...prev];
            const previousCursor = next.pop();
            setCursor(previousCursor || undefined);
            return next;
          });
        }}
        onNext={() => {
          if (!meta.next_cursor) return;
          setCursorStack((prev) => [...prev, cursor ?? '']);
          setCursor(meta.next_cursor);
        }}
        onLimitChange={(nextLimit) => {
          setMeta((prev) => ({ ...prev, limit: nextLimit }));
          setCursor(undefined);
          setCursorStack([]);
        }}
      />
      <div className="table-wrapper" role="region" tabIndex={0} aria-label="Groups table">
        {loading ? (
          <p className="muted">Loading groups…</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Group</th>
                <th>Members</th>
                <th>Roles</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {groups.map((entry) => (
                <tr key={entry.group.id}>
                  <td>
                    <strong>{entry.group.name}</strong>
                    <div className="muted">{entry.group.description || 'No description'}</div>
                  </td>
                  <td>
                    <div className="chip-row">
                      {entry.members.map((member) => (
                        <span key={member.id} className="chip subtle">
                          {member.display_name || member.email}
                          <button
                            type="button"
                            onClick={() => removeMember(entry.group.id, member.id)}
                            aria-label="Remove member"
                          >
                            ×
                          </button>
                        </span>
                      ))}
                    </div>
                    <div className="inline-form">
                      <select
                        value={memberSelection[entry.group.id] || ''}
                        onChange={(e) =>
                          setMemberSelection((prev) => ({
                            ...prev,
                            [entry.group.id]: e.target.value,
                          }))
                        }
                      >
                        <option value="">Add member…</option>
                        {directoryOptions.map((option) => (
                          <option key={option.id} value={option.id}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                      <button
                        type="button"
                        className="ghost-button"
                        onClick={() => addMember(entry.group.id)}
                      >
                        Add
                      </button>
                    </div>
                  </td>
                  <td>
                    <div className="chip-row">
                      {entry.roles.map((role) => (
                        <span key={role} className="chip">
                          {role}
                          <button type="button" onClick={() => revokeRole(entry.group.id, role)}>
                            ×
                          </button>
                        </span>
                      ))}
                    </div>
                    <div className="inline-form">
                      <select
                        value={roleSelection[entry.group.id] || ''}
                        onChange={(e) =>
                          setRoleSelection((prev) => ({
                            ...prev,
                            [entry.group.id]: e.target.value,
                          }))
                        }
                      >
                        <option value="">Assign role…</option>
                        {roleCatalog.map((role) => (
                          <option key={role.id} value={role.name}>
                            {role.name}
                          </option>
                        ))}
                      </select>
                      <button
                        type="button"
                        className="ghost-button"
                        onClick={() => assignRole(entry.group.id)}
                      >
                        Assign
                      </button>
                    </div>
                  </td>
                  <td>
                    <button className="ghost-button" onClick={() => deleteGroup(entry.group.id)}>
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
};

const IamRolesPanel = () => {
  const { roles, loading, error, reload } = useIamRoles();
  return (
    <section>
      <header className="iam-section-head">
        <div>
          <h3>Role Catalog</h3>
          <p>Inspect built-in roles and the permissions they grant across the platform.</p>
        </div>
        <button className="ghost-button" onClick={reload}>
          Refresh
        </button>
      </header>
      {error && <div className="error-banner">{error}</div>}
      <div className="table-wrapper" role="region" tabIndex={0} aria-label="Roles table">
        {loading ? (
          <p className="muted">Loading roles…</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Role</th>
                <th>Permissions</th>
              </tr>
            </thead>
            <tbody>
              {roles.map((role) => (
                <tr key={role.id}>
                  <td>
                    <strong>{role.name}</strong>
                    <div className="muted">{role.description || 'No description'}</div>
                  </td>
                  <td>
                    <div className="chip-row">
                      {role.permissions.map((perm) => (
                        <span key={perm} className="chip subtle">
                          {perm}
                        </span>
                      ))}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
};

const IamServiceAccountsPanel = () => {
  const api = useAdminApi();
  const { roles } = useIamRoles();
  const [accounts, setAccounts] = useState<ServiceAccountDetails[]>([]);
  const [meta, setMeta] = useState<CursorMeta>({ limit: 50, has_more: false });
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const [lastToken, setLastToken] = useState<ServiceAccountWithToken>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [form, setForm] = useState({ name: '', description: '', roles: [] as string[] });

  const loadAccounts = useCallback(async () => {
    if (!api.canCallApi) return;
    setLoading(true);
    setError(undefined);
    try {
      const body = await adminGetJson<CursorPaged<ServiceAccountDetails>>(
        api as AdminApiContext,
        '/api/v1/iam/service-accounts',
        { limit: meta.limit, cursor },
      );
      setAccounts(body.data);
      setMeta(body.meta);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load service accounts');
    } finally {
      setLoading(false);
    }
  }, [api, cursor, meta.limit]);

  useEffect(() => {
    loadAccounts();
  }, [loadAccounts]);

  const toggleRole = (role: string) => {
    setForm((prev) => {
      const hasRole = prev.roles.includes(role);
      return {
        ...prev,
        roles: hasRole ? prev.roles.filter((r) => r !== role) : [...prev.roles, role],
      };
    });
  };

  const createServiceAccount = async (event: FormEvent) => {
    event.preventDefault();
    if (!form.name.trim()) {
      setError('Name is required');
      return;
    }
    try {
      const result = await adminPostJson<ServiceAccountWithToken>(
        api as AdminApiContext,
        '/api/v1/iam/service-accounts',
        {
          name: form.name.trim(),
          description: form.description.trim() || undefined,
          roles: form.roles,
        },
      );
      setLastToken(result);
      setForm({ name: '', description: '', roles: [] });
      loadAccounts();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create service account');
    }
  };

  const rotateServiceAccount = async (id: string, roles?: string[]) => {
    try {
      const result = await adminPostJson<ServiceAccountWithToken>(
        api as AdminApiContext,
        `/api/v1/iam/service-accounts/${id}/rotate`,
        {
          roles,
        },
      );
      setLastToken(result);
      loadAccounts();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to rotate token');
    }
  };

  const disableServiceAccount = async (id: string) => {
    try {
      await adminDelete(api as AdminApiContext, `/api/v1/iam/service-accounts/${id}`);
      loadAccounts();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to disable service account');
    }
  };

  return (
    <section>
      <header className="iam-section-head">
        <div>
          <h3>Service Accounts</h3>
          <p>Issue scoped access tokens for automation, CI/CD pipelines, or secure integrations.</p>
        </div>
      </header>
      <form className="iam-form" onSubmit={createServiceAccount}>
        <div className="iam-form-grid">
          <label>
            <span>Name</span>
            <input
              value={form.name}
              onChange={(e) => setForm((prev) => ({ ...prev, name: e.target.value }))}
              placeholder="policy-ci"
              required
            />
          </label>
          <label>
            <span>Description</span>
            <input
              value={form.description}
              onChange={(e) => setForm((prev) => ({ ...prev, description: e.target.value }))}
              placeholder="CI/CD deploy pipeline"
            />
          </label>
        </div>
        <div className="iam-role-checkboxes">
          {roles.map((role) => (
            <label key={role.id} className="checkbox-pill">
              <input
                type="checkbox"
                checked={form.roles.includes(role.name)}
                onChange={() => toggleRole(role.name)}
              />
              {role.name}
            </label>
          ))}
        </div>
        <button className="cta-button" disabled={!api.canCallApi}>Generate Token</button>
      </form>
      {lastToken && (
        <div className="glass-panel" style={{ marginTop: '1rem', borderColor: 'rgba(86,196,255,0.3)' }}>
          <p className="section-title">New token for {lastToken.account.name}</p>
          <code className="token-display">{lastToken.token}</code>
          <p className="muted" style={{ marginTop: '0.35rem' }}>
            Copy now — this token will not be shown again.
          </p>
        </div>
      )}
      {error && <div className="error-banner">{error}</div>}
      <PaginationControls
        limit={meta.limit}
        loading={loading}
        hasMore={Boolean(meta.next_cursor) && meta.has_more}
        canGoBack={cursorStack.length > 0}
        onPrev={() => {
          setCursorStack((prev) => {
            if (prev.length === 0) return prev;
            const next = [...prev];
            const previousCursor = next.pop();
            setCursor(previousCursor || undefined);
            return next;
          });
        }}
        onNext={() => {
          if (!meta.next_cursor) return;
          setCursorStack((prev) => [...prev, cursor ?? '']);
          setCursor(meta.next_cursor);
        }}
        onLimitChange={(nextLimit) => {
          setMeta((prev) => ({ ...prev, limit: nextLimit }));
          setCursor(undefined);
          setCursorStack([]);
        }}
      />
      <div className="table-wrapper" role="region" tabIndex={0} aria-label="Service accounts table">
        {loading ? (
          <p className="muted">Loading service accounts…</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Account</th>
                <th>Roles</th>
                <th>Token hint</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {accounts.map((entry) => (
                <tr key={entry.account.id}>
                  <td>
                    <strong>{entry.account.name}</strong>
                    <div className="muted">{entry.account.description || 'No description'}</div>
                  </td>
                  <td>
                    <div className="chip-row">
                      {entry.roles.map((role) => (
                        <span key={role} className="chip subtle">
                          {role}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td>{entry.account.token_hint || '—'}</td>
                  <td>
                    <div className="button-stack">
                      <button
                        className="ghost-button"
                        onClick={() => rotateServiceAccount(entry.account.id)}
                      >
                        Rotate
                      </button>
                      <button
                        className="ghost-button"
                        onClick={() => disableServiceAccount(entry.account.id)}
                      >
                        Disable
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
};

const IamAuditPanel = () => {
  const api = useAdminApi();
  const [events, setEvents] = useState<IamAuditRecord[]>([]);
  const [meta, setMeta] = useState<CursorMeta>({ limit: 100, has_more: false });
  const [cursor, setCursor] = useState<string | undefined>();
  const [cursorStack, setCursorStack] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  const loadEvents = useCallback(async () => {
    if (!api.canCallApi) return;
    setLoading(true);
    setError(undefined);
    try {
      const body = await adminGetJson<CursorPaged<IamAuditRecord>>(
        api as AdminApiContext,
        '/api/v1/iam/audit',
        { limit: meta.limit, cursor },
      );
      setEvents(body.data);
      setMeta(body.meta);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load audit events');
    } finally {
      setLoading(false);
    }
  }, [api, cursor, meta.limit]);

  useEffect(() => {
    loadEvents();
  }, [loadEvents]);

  return (
    <section>
      <header className="iam-section-head">
        <div>
          <h3>Audit Evidence</h3>
          <p>Immutable audit stream for IAM mutations, suitable for compliance exports.</p>
        </div>
        <button className="ghost-button" onClick={loadEvents}>
          Refresh
        </button>
      </header>
      {error && <div className="error-banner">{error}</div>}
      <PaginationControls
        limit={meta.limit}
        loading={loading}
        hasMore={Boolean(meta.next_cursor) && meta.has_more}
        canGoBack={cursorStack.length > 0}
        onPrev={() => {
          setCursorStack((prev) => {
            if (prev.length === 0) return prev;
            const next = [...prev];
            const previousCursor = next.pop();
            setCursor(previousCursor || undefined);
            return next;
          });
        }}
        onNext={() => {
          if (!meta.next_cursor) return;
          setCursorStack((prev) => [...prev, cursor ?? '']);
          setCursor(meta.next_cursor);
        }}
        onLimitChange={(nextLimit) => {
          setMeta((prev) => ({ ...prev, limit: nextLimit }));
          setCursor(undefined);
          setCursorStack([]);
        }}
      />
      <div className="table-wrapper" role="region" tabIndex={0} aria-label="Audit log table">
        {loading ? (
          <p className="muted">Loading events…</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>When</th>
                <th>Actor</th>
                <th>Action</th>
                <th>Target</th>
                <th>Payload</th>
              </tr>
            </thead>
            <tbody>
              {events.map((event) => (
                <tr key={event.id}>
                  <td>{new Date(event.created_at).toLocaleString()}</td>
                  <td>{event.actor || 'system'}</td>
                  <td>{event.action}</td>
                  <td>
                    {event.target_type ? `${event.target_type} · ${event.target_id ?? '—'}` : '—'}
                  </td>
                  <td>
                    <code className="payload-snippet">
                      {JSON.stringify(event.payload ?? {}, null, 2)}
                    </code>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
};

export default SettingsIamPage;
