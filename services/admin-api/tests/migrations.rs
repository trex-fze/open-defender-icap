use anyhow::Result;
use sqlx::{postgres::PgPoolOptions, Row};

#[tokio::test]
async fn migrations_apply_and_seed_roles() -> Result<()> {
    let db_url = match std::env::var("ADMIN_TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!(
                "skipping migrations_apply_and_seed_roles (set ADMIN_TEST_DATABASE_URL to run)"
            );
            return Ok(());
        }
    };

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await?;

    // Apply the full migration stack to ensure the IAM tables exist.
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;

    let role_row = sqlx::query("SELECT COUNT(*) as count FROM iam_roles")
        .fetch_one(&pool)
        .await?;
    let role_count: i64 = role_row.get("count");
    assert!(role_count >= 5, "expected at least 5 seeded roles, saw {role_count}");

    let perm_row = sqlx::query(
        "SELECT COUNT(*) as count FROM iam_role_permissions WHERE permission = 'iam:manage'",
    )
    .fetch_one(&pool)
    .await?;
    let perm_count: i64 = perm_row.get("count");
    assert!(perm_count >= 1, "expected iam:manage permission to be seeded");

    Ok(())
}
