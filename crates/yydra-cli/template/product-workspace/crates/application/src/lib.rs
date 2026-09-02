// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strongly typed Product Workspace use cases belong here.

#![forbid(unsafe_code)]

use product_persistence_postgres::Database;

#[derive(Clone)]
pub struct HealthService {
    database: Database,
}

impl HealthService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn check(&self) -> Result<HealthStatus, Box<dyn std::error::Error>> {
        Ok(HealthStatus {
            status: "ready",
            database: self.database.schema_name().await?,
        })
    }
}

pub struct HealthStatus {
    pub status: &'static str,
    pub database: String,
}
