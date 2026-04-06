use assert_cmd::Command;
use dirs::config_dir;
use predicates::prelude::*;
use serde_json::json;
use std::{env, fs};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policy_list_hits_admin_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/policies"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "id": "73c3c89b-157f-40c6-9c5b-6dfb4f9e5b3c",
                    "name": "Live Policy",
                    "version": "v42",
                    "status": "active",
                    "rule_count": 7
                }
            ],
            "meta": {"page": 1, "page_size": 50, "total": 1, "has_more": false}
        })))
        .mount(&server)
        .await;

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args(["policy", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Live Policy"))
        .stdout(predicate::str::contains("v42"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policy_runtime_sync_hits_admin_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/policies/runtime-sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "control_plane": {
                "policy_id": "73c3c89b-157f-40c6-9c5b-6dfb4f9e5b3c",
                "version": "release-20260406"
            },
            "runtime": {
                "policy_id": "73c3c89b-157f-40c6-9c5b-6dfb4f9e5b3c",
                "version": "release-20260406"
            },
            "in_sync": true,
            "drift_reason": null
        })))
        .mount(&server)
        .await;

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args(["policy", "runtime-sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Runtime sync: in-sync"))
        .stdout(predicate::str::contains("release-20260406"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policy_create_validates_then_posts_admin_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/policies/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "valid": true,
            "errors": []
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/policies"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "73c3c89b-157f-40c6-9c5b-6dfb4f9e5b3c",
            "name": "CLI Draft",
            "version": "v99",
            "status": "draft",
            "rule_count": 1,
            "rules": [
                {
                    "id": "r1",
                    "description": "Monitor social",
                    "priority": 10,
                    "action": "Monitor",
                    "conditions": {"categories": ["social-media"]}
                }
            ]
        })))
        .mount(&server)
        .await;

    let temp = TempDir::new().unwrap();
    let file = temp.path().join("policy.json");
    fs::write(
        &file,
        serde_json::to_string_pretty(&json!({
            "version": "v99",
            "rules": [
                {
                    "id": "r1",
                    "priority": 10,
                    "action": "Monitor",
                    "description": "Monitor social",
                    "conditions": {"categories": ["social-media"]}
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args([
            "policy",
            "create",
            "--file",
            file.to_str().unwrap(),
            "--name",
            "CLI Draft",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created policy CLI Draft"))
        .stdout(predicate::str::contains("version v99"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policy_history_hits_admin_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/policies/73c3c89b-157f-40c6-9c5b-6dfb4f9e5b3c/versions",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": "5a5dcd8b-14ce-4d6a-8f2b-f69894f50111",
                "policy_id": "73c3c89b-157f-40c6-9c5b-6dfb4f9e5b3c",
                "version": "release-20260406",
                "status": "active",
                "created_by": "ci",
                "created_at": "2026-04-06T10:00:00Z",
                "deployed_at": "2026-04-06T10:01:00Z",
                "notes": "published",
                "rule_count": 7
            }
        ])))
        .mount(&server)
        .await;

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args(["policy", "history", "73c3c89b-157f-40c6-9c5b-6dfb4f9e5b3c"])
        .assert()
        .success()
        .stdout(predicate::str::contains("release-20260406"))
        .stdout(predicate::str::contains("active"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn override_list_hits_admin_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/overrides"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "id": "e1c3a558-037f-4e32-874c-a076e7d0b258",
                    "scope_type": "domain",
                    "scope_value": "example.com",
                    "action": "allow",
                    "status": "active",
                    "expires_at": null
                }
            ],
            "meta": {"limit": 50, "has_more": false, "next_cursor": null, "prev_cursor": null}
        })))
        .mount(&server)
        .await;

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args(["override", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("domain:example.com"))
        .stdout(predicate::str::contains("allow"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iam_users_list_prints_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/iam/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "user": {
                        "id": "0c2f2b71-9ab6-4f39-905a-0b2d4f0a1111",
                        "email": "avery@example.com",
                        "display_name": "Avery Quinn",
                        "subject": null,
                        "status": "active",
                        "created_at": "2026-03-24T00:00:00Z",
                        "updated_at": "2026-03-24T00:00:00Z",
                        "last_login_at": null
                    },
                    "roles": ["policy-admin"],
                    "groups": []
                }
            ],
            "meta": {"limit": 100, "has_more": false, "next_cursor": null, "prev_cursor": null}
        })))
        .mount(&server)
        .await;

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args(["iam", "users", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("avery@example.com"))
        .stdout(predicate::str::contains("policy-admin"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iam_whoami_json_output() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/iam/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "actor": "svc-ci",
            "principal_type": "service_account",
            "principal_id": "b2a49811-58d0-42f3-a5d8-d6e2a5c2f9ab",
            "roles": ["policy-editor"],
            "permissions": ["iam:manage"]
        })))
        .mount(&server)
        .await;

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args(["--json", "iam", "whoami"])
        .assert()
        .success()
        .stdout(predicate::str::contains("svc-ci"))
        .stdout(predicate::str::contains("iam:manage"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iam_service_account_create_outputs_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/iam/service-accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "account": {
                "id": "5bdb4bda-0f5c-4a0e-8ac4-3fa321312222",
                "name": "deploy-bot",
                "description": "",
                "status": "active",
                "token_hint": "xyz12345",
                "created_at": "2026-03-24T00:00:00Z",
                "last_rotated_at": "2026-03-24T00:00:00Z"
            },
            "token": "svc.token.value",
            "roles": ["policy-editor"]
        })))
        .mount(&server)
        .await;

    Command::cargo_bin("odctl")
        .unwrap()
        .arg("--base-url")
        .arg(server.uri())
        .arg("--token")
        .arg("static")
        .args([
            "iam",
            "service-accounts",
            "create",
            "--name",
            "deploy-bot",
            "--role",
            "policy-editor",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("svc.token.value"))
        .stdout(predicate::str::contains("deploy-bot"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_login_device_flow_stores_session() {
    let oidc = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dev-123",
            "user_code": "ABCD-1234",
            "verification_uri": "https://login.example/device",
            "verification_uri_complete": "https://login.example/device?user_code=ABCD-1234",
            "expires_in": 600,
            "interval": 1
        })))
        .mount(&oidc)
        .await;

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "token-abc",
            "refresh_token": "refresh-xyz",
            "token_type": "Bearer",
            "expires_in": 3600
        })))
        .mount(&oidc)
        .await;

    let temp_home = TempDir::new().unwrap();
    let prev_config = env::var("XDG_CONFIG_HOME").ok();
    let prev_home = env::var("HOME").ok();
    env::set_var("XDG_CONFIG_HOME", temp_home.path());
    env::set_var("HOME", temp_home.path());
    let device_url = format!("{}/device", oidc.uri());
    let token_url = format!("{}/token", oidc.uri());

    Command::cargo_bin("odctl")
        .unwrap()
        .args([
            "auth",
            "login",
            "--client-id",
            "test-cli",
            "--device-url",
            &device_url,
            "--token-url",
            &token_url,
            "--scope",
            "openid profile",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Authentication successful"));

    let session_path = config_dir()
        .expect("config dir")
        .join("odctl")
        .join("session.json");
    let session = fs::read_to_string(&session_path).expect("session file");
    let parsed: serde_json::Value = serde_json::from_str(&session).unwrap();
    assert_eq!(parsed["access_token"], "token-abc");
    assert_eq!(parsed["refresh_token"], "refresh-xyz");

    if let Some(value) = prev_config {
        env::set_var("XDG_CONFIG_HOME", value);
    } else {
        env::remove_var("XDG_CONFIG_HOME");
    }
    if let Some(value) = prev_home {
        env::set_var("HOME", value);
    } else {
        env::remove_var("HOME");
    }
}
