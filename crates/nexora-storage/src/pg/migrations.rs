//! PostgreSQL schema initialization.
//!
//! The schema is embedded as a single SQL string and executed on first
//! connection. All statements use `IF NOT EXISTS` so re-runs are safe.

/// The full v1 schema. Applies in one batch.
pub const V1_SCHEMA: &str = include_str!("../../migrations/V1__initial_schema.sql");

/// Apply the schema to the given pool. Idempotent.
pub async fn apply_schema(pool: &crate::pg::PgPool) -> Result<(), crate::pg::PgError> {
    // Split on `;` to execute each statement separately (tokio-postgres
    // does not support multi-statement queries in one execute() call).
    for stmt in V1_SCHEMA.split(';') {
        let trimmed = stmt.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        pool.execute(trimmed, &[]).await?;
    }
    Ok(())
}

/// Synchronous migration runner — for use in non-async contexts. Currently
/// unused but kept for future CLI integration.
pub fn run_migrations(_database_url: &str) -> Result<(), String> {
    // In the async-only model, callers should use `apply_schema(&pool).await`.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_compiles_and_is_nonempty() {
        assert!(V1_SCHEMA.len() > 100);
        assert!(V1_SCHEMA.contains("CREATE TABLE"));
        assert!(V1_SCHEMA.contains("users"));
        assert!(V1_SCHEMA.contains("events"));
        assert!(V1_SCHEMA.contains("packages"));
    }
}
