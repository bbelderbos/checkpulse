use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

pub async fn connect(database_path: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
