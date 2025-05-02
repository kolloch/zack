use std::sync::Arc;

use ahash::AHashMap;
use diesel::{associations::HasTable, query_dsl::methods::SelectDsl, RunQueryDsl, SelectableHelper};

use crate::{db::{Db, DbHelper}, model::{BuildConfig, BuildConfigId, NewBuildConfig}};
use anyhow::Result;

#[derive(Default)]
struct BuildConfigCache {
    configs: AHashMap<BuildConfigId, BuildConfig>,
}

impl DbHelper for BuildConfigCache {
    fn initialize(&mut self, db: &mut crate::db::Db) -> anyhow::Result<()> {
        use crate::schema::build_configs::dsl::*;

        let conn = db.connection();
        let configs = build_configs.select(BuildConfig::as_select()).get_results(conn)?;
        
        if !configs.is_empty() {
            self.configs = configs.into_iter().map(|c| (c.id, c)).collect();
            return Ok(())
        }

        let default_config: BuildConfig =  diesel::insert_into(build_configs::table())
            .values(&NewBuildConfig {
                name: "default",
            })
            .returning(BuildConfig::as_returning())
            .get_result(conn)?;

        self.configs.insert(default_config.id, default_config);

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
    fn get_default_build_config(&mut self) -> Result<CacheAccess> {
        self.get_build_config(BuildConfigId::DEFAULT_CONFIG_ID)
    }

    fn get_build_config(&mut self, id: BuildConfigId) -> Result<CacheAccess>;
}

impl BuildConfigsDAO for Db {
    fn get_build_config(&mut self, id: BuildConfigId) -> Result<CacheAccess> {
        let cache: Arc<BuildConfigCache> = self.helper()?;
        Ok(CacheAccess { cache, id })
    }
}