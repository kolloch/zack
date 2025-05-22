use std::sync::Arc;

use ahash::AHashMap;
use sea_orm::{ActiveModelTrait, EntityTrait};

use crate::{
    db::{Db, DbHelper},
    entity::build_config,
    model::{BuildConfig, BuildConfigId},
};
use anyhow::Result;

#[derive(Default)]
struct BuildConfigCache {
    configs: AHashMap<BuildConfigId, BuildConfig>,
}

impl DbHelper for BuildConfigCache {
    async fn initialize(&mut self, db: &mut crate::db::Db) -> anyhow::Result<()> {
        use crate::entity::prelude::*;

        let conn = db.connection();

        let configs = BuildConfig::find().all(conn).await?;

        if configs.is_empty() {
            let default_config = build_config::ActiveModel {
                name: sea_orm::ActiveValue::Set("default".into()),
                ..Default::default()
            };
            let result = default_config.insert(conn).await?;
            let id = BuildConfigId(result.id);
            let build_config = crate::model::BuildConfig {
                id: id.clone(),
                name: result.name,
            };
            self.configs.insert(id, build_config);
        } else {
            self.configs = configs
                .into_iter()
                .map(|m| {
                    (
                        BuildConfigId(m.id),
                        crate::model::BuildConfig {
                            id: BuildConfigId(m.id),
                            name: m.name,
                        },
                    )
                })
                .collect();
        }

        Ok(())
    }
}

pub struct CacheAccess {
    cache: Arc<BuildConfigCache>,
    id: BuildConfigId,
}

impl CacheAccess {
    pub fn get(&self) -> Option<&BuildConfig> {
        self.cache.configs.get(&self.id)
    }
}

pub trait BuildConfigsDAO {
    async fn get_default_build_config(&mut self) -> Result<CacheAccess> {
        self.get_build_config(BuildConfigId::DEFAULT_CONFIG_ID)
            .await
    }

    async fn get_build_config(&mut self, id: BuildConfigId) -> Result<CacheAccess>;
}

impl BuildConfigsDAO for Db {
    async fn get_build_config(&mut self, id: BuildConfigId) -> Result<CacheAccess> {
        let cache: Arc<BuildConfigCache> = self.helper().await?;
        Ok(CacheAccess { cache, id })
    }
}
