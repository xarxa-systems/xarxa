use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use validator::Validate;

pub type Token = String;

#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[sqlx(rename = "token_hash")]
    pub token: Token,
    pub super_admin: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct NewUser {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct NewUserResponse {
    pub password: String,
}

#[derive(Clone, Deserialize, Validate)]
pub struct UserAuth {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 24))]
    pub password: Token,
}