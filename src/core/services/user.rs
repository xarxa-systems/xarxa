use std::sync::Arc;
use anyhow::{bail, Result};
use rand::{self, Rng};
use rand::distr::Alphanumeric;
use sha2::{Digest, Sha256};

use crate::core::ports::storage::UserRepository;
use crate::core::domain::user::{NewUser, User};


pub struct UserService{
    repo: Arc<dyn UserRepository>,
}

impl UserService {
    #[cold]
    pub fn new(r: Arc<dyn UserRepository>) -> Self {
        UserService{
            repo: r,
        }
    }

    pub async fn auth(&self, email: String, token: String) -> Result<User> {
        match self.repo.find_by_email(&email).await? {
            Some(u) => {
                if verify_token(&token, &u.token).await {
                   Ok(u)
                } else {
                    bail!("Wrong email or password")
                }
            },
            None => bail!("Wrong email or password")
        }
    }

    pub async fn create(&self, u: &NewUser) -> Result<User, anyhow::Error> {
        let real_token: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(64)
            .map(char::from)
            .collect();

        let salt: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let mut hasher = Sha256::new();
        hasher.update(real_token.as_bytes());
        hasher.update(salt.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        
        let token_hash = format!("{}:{}", salt, hash);

        let mut u = self.repo.create(u, token_hash).await?;
        u.token = real_token;

        Ok(u)
    }
}

#[cold]
async fn verify_token(token: &str, stored_hash: &str) -> bool {
    let parts: Vec<&str> = stored_hash.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    
    let salt = parts[0];
    let hash = parts[1];
    
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.update(salt.as_bytes());
    let computed_hash = format!("{:x}", hasher.finalize());
    
    computed_hash == hash
}