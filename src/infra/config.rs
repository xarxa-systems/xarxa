/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * Xarxa is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0).
 * See the LICENSE file in the project root for the full license text.
 *
 * Commercial licensing (MIT / proprietary) is available.
 * Contact: contact@xarxa.io
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use std::sync::Arc;

use serde::Deserialize;
use aws_credential_types::{
    provider::{self, future, ProvideCredentials},
    Credentials,
};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub workflows_default_path: String,
    pub port: u16,
    pub log_level: String,
    pub environment: String,
    pub space_key: String,
    pub space_secret: String,
    pub bucket_name: String,
    pub space_endpoint: String,
}

impl ProvideCredentials for AppConfig {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a> where Self: 'a {
        future::ProvideCredentials::new(self.load_credentials())
    }
    
    fn fallback_on_interrupt(&self) -> Option<Credentials> {
        None
    }
}

impl AppConfig {
    pub async fn load_credentials(&self) -> provider::Result {
        Ok(Credentials::new(&self.space_key, &self.space_secret, None, None, "DigitalOcean"))
    }

    pub fn from_env() -> Result<Self, config::ConfigError> {
        dotenvy::dotenv().ok();

        let cfg = config::Config::builder()
            .add_source(config::Environment::default())
            .build()?;

        cfg.try_deserialize()
    }
}