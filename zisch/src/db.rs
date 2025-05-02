use std::sync::Arc;

use anymap::AnyMap;
use diesel::prelude::*;
use anyhow::Result;

pub trait DbHelper {
    fn initialize(&mut self, db: &mut Db) -> anyhow::Result<()>;
}

pub struct Db {
    con: SqliteConnection,
    caches: AnyMap,
}

impl Db {
    pub fn new() -> Result<Db> {
        let con = connection();
        let caches = AnyMap::new();

        Ok(Db {
            con,
            caches,
        })
    }

    pub fn connection(&mut self) -> &mut SqliteConnection {
        return &mut self.con;
    }

    pub fn helper<DBH: DbHelper+Default+'static>(&mut self) -> Result<Arc<DBH>> {
        if let Some(existing) = self.caches.get::<Arc<DBH>>() {
            return Ok(existing.clone());
        }    

        let mut new_dbh = DBH::default();
        new_dbh.initialize(self)?;
        let new_dbh = Arc::new(new_dbh);

        self.caches.insert(new_dbh.clone());

        Ok(new_dbh)
    }
}

fn connection() -> SqliteConnection {
    let db_file = directories::db();
    SqliteConnection::establish(db_file.as_str())
        .unwrap_or_else(|_| panic!("Error connecting to {}", db_file))
}