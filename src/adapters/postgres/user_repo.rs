use async_trait::async_trait;
use std::sync::Arc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::core::domain::user::{NewUser, User, Token};
use crate::core::ports::storage::UserRepository;

pub struct PostgresUserRepository {
    pool: Arc<PgPool>,
}

impl PostgresUserRepository {
    pub fn new(pool: Arc<PgPool>) -> impl UserRepository {
        PostgresUserRepository {
            pool,
        }
    }
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    async fn create(&self, u: &NewUser, token_hash: Token) -> Result<User, anyhow::Error> {
        let id = Uuid::now_v7();

        let row = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (id, email, token_hash)
            VALUES ($1, $2, $3)
            RETURNING id, email, token_hash as token, super_admin, created_at
            "#,
            id,
            u.email,
            token_hash,
        )
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some(user) => Ok(user),
            None => Err(anyhow::anyhow!("failed to create user")),
        }
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, anyhow::Error> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, token_hash as token, super_admin, created_at
            FROM users
            WHERE email = $1
            "#,
            email
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(user)
    }
}