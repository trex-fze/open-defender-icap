import { rbacMatrix } from '../data/mockData';

export const SettingsRbacPage = () => {
  return (
    <div>
      <div className="page-header">
        <div>
          <p className="section-title">RBAC</p>
          <h2 style={{ margin: 0 }}>Role assignments</h2>
        </div>
        <button className="cta-button">Invite User</button>
      </div>

      <div className="glass-panel">
        <div className="table-wrapper">
          <table>
            <thead>
              <tr>
                <th>User</th>
                <th>Email</th>
                <th>Roles</th>
              </tr>
            </thead>
            <tbody>
              {rbacMatrix.map((user) => (
                <tr key={user.email}>
                  <td>{user.name}</td>
                  <td>{user.email}</td>
                  <td>
                    {user.roles.map((role) => (
                      <span key={role} className="chip" style={{ marginRight: '0.35rem' }}>
                        {role}
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
