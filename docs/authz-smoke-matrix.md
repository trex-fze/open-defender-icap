# AuthZ Smoke Matrix

Use this sheet as a quick regression checklist whenever we change IAM or route guards. The table assumes the Admin API is reachable at `http://localhost:19000` and that you have:

* `VIEWER_BEARER` → local login token from a `policy-viewer` user.
* `ADMIN_BEARER` → local login token from bootstrap `admin` or any `policy-admin` user.

| Endpoint | Unauthenticated | Viewer Bearer | Admin Bearer |
| --- | --- | --- | --- |
| `POST /api/v1/auth/login` | `401`/`400` | `200` with token | `200` with token |
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

# Login as viewer/admin to mint bearer tokens
VIEWER_BEARER=$(curl -s -X POST http://localhost:19000/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"viewer","password":"viewer-password"}' | jq -r '.access_token')

ADMIN_BEARER=$(curl -s -X POST http://localhost:19000/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"'$OD_DEFAULT_ADMIN_PASSWORD'"}' | jq -r '.access_token')

# Viewer should see overrides but not create them
curl -i -H "Authorization: Bearer $VIEWER_BEARER" http://localhost:19000/api/v1/overrides | head -n 1
curl -i -H "Authorization: Bearer $VIEWER_BEARER" -X POST http://localhost:19000/api/v1/overrides \
  -d '{"scope_type":"domain","scope_value":"example.com","action":"allow"}' \
  -H 'Content-Type: application/json' | head -n 1

# Admin can manage IAM entries
curl -H "Authorization: Bearer $ADMIN_BEARER" http://localhost:19000/api/v1/iam/users
curl -H "Authorization: Bearer $ADMIN_BEARER" -X POST http://localhost:19000/api/v1/iam/users \
  -d '{"email":"smoke+user@example.com"}' -H 'Content-Type: application/json'
```

Document the responses in release notes whenever we make auth changes.
