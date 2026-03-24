use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::test]
async fn iam_audit_rejects_non_object_payloads() -> Result<()> {
    let db_url = match std::env::var("ADMIN_TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!(
                "skipping iam_audit_rejects_non_object_payloads (set ADMIN_TEST_DATABASE_URL to run)"
            );
            return Ok(());
        }
    };

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let bad = sqlx::query(
        "INSERT INTO iam_audit_events (id, action, payload) VALUES ($1, 'iam.test', ($2)::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind("\"bogus\"")
    .execute(&pool)
    .await;
    assert!(bad.is_err(), "expected payload constraint to reject string values");

    Ok(())
}
