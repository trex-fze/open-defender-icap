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
async fn override_list_hits_admin_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/overrides"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": "e1c3a558-037f-4e32-874c-a076e7d0b258",
                "scope_type": "domain",
                "scope_value": "example.com",
                "action": "allow",
                "status": "active",
                "expires_at": null
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
        .args(["override", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("domain:example.com"))
        .stdout(predicate::str::contains("allow"));
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
