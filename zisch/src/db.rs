use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use anymap2::AnyMap;
use migration::MigratorTrait;
use sea_orm::{Database, DatabaseConnection};
use url::Url;

pub trait DbHelper {
    async fn initialize(&mut self, db: &mut Db) -> anyhow::Result<()>;
}

pub struct Db {
    connection: DatabaseConnection,
    caches: AnyMap,
}

impl Db {
    pub async fn new() -> Result<Db> {
        let db_file = directories::db();
        let db_file_url = Url::from_file_path(db_file)
            .map_err(|_| anyhow!("Could not create URL from {db_file}"))?;
        let db_file_path = db_file_url.path();
        let database = Database::connect(format!("sqlite:{db_file_path}"))
            .await
            .with_context(|| format!("while opening {db_file}"))?;

        // FIXME: just once per startup
        migration::Migrator::up(&database, None).await?;

        let caches = AnyMap::new();

        Ok(Db {
            connection: database,
            caches,
        })
    }

    pub fn connection(&self) -> &DatabaseConnection {
        &self.connection
    }

    pub async fn helper<DBH: DbHelper + Default + 'static>(&mut self) -> Result<Arc<DBH>> {
        if let Some(existing) = self.caches.get::<Arc<DBH>>() {
            return Ok(existing.clone());
        }

        let mut new_dbh = DBH::default();
        new_dbh.initialize(self).await?;
        let new_dbh = Arc::new(new_dbh);

        self.caches.insert(new_dbh.clone());

        Ok(new_dbh)
    }
}
