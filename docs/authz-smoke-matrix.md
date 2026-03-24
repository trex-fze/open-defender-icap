# AuthZ Smoke Matrix

Use this sheet as a quick regression checklist whenever we change IAM or route guards. The table assumes the Admin API is reachable at `http://localhost:19000` and that you have two tokens:

* `VIEWER_TOKEN` → user with `policy-viewer` + `iam:view`
* `ADMIN_TOKEN` → service account with `policy-admin`

| Endpoint | Unauthenticated | Viewer Token | Admin Token |
| --- | --- | --- | --- |
| `GET /api/v1/overrides` | `401` (missing header) | `200` list returned | `200` |
| `POST /api/v1/overrides` | `401` | `403` (viewer) | `201` |
| `GET /api/v1/iam/users` | `401` | `200` (read-only) | `200` |
| `POST /api/v1/iam/users` | `401` | `403` | `201` |
| `POST /api/v1/iam/users/:id/roles` | `401` | `403` | `200` |
| `GET /api/v1/iam/audit` | `401` | `403` (unless viewer also `auditor`) | `200` |
| `GET /api/v1/iam/whoami` | `401` | `200` | `200` |

### Smoke Script

```bash
# Unauthenticated should be 401
curl -i http://localhost:19000/api/v1/overrides | head -n 1

# Viewer should see overrides but not create them
curl -i -H "X-Admin-Token: $VIEWER_TOKEN" http://localhost:19000/api/v1/overrides | head -n 1
curl -i -H "X-Admin-Token: $VIEWER_TOKEN" -X POST http://localhost:19000/api/v1/overrides \
  -d '{"scope_type":"domain","scope_value":"example.com","action":"allow"}' \
  -H 'Content-Type: application/json' | head -n 1

# Admin can manage IAM entries
curl -H "X-Admin-Token: $ADMIN_TOKEN" http://localhost:19000/api/v1/iam/users
curl -H "X-Admin-Token: $ADMIN_TOKEN" -X POST http://localhost:19000/api/v1/iam/users \
  -d '{"email":"smoke+user@example.com"}' -H 'Content-Type: application/json'
```

Document the responses in release notes whenever we make auth changes.
