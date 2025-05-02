use std::ops::Deref;

use anyhow::Context;
use diesel::{backend::SqlDialect, connection::DefaultLoadingMode, query_builder::{AsQuery, Query, QueryFragment, QueryId}, query_dsl::methods::LoadQuery, sqlite::Sqlite, Connection, ExpressionMethods, QueryDsl, QueryResult, QuerySource, Queryable, RunQueryDsl, Selectable, SqliteConnection};

use crate::{db::Db, model::{File, FileKind}};
use crate::schema::files::dsl::*;

const PAGE_SIZE: i64 = 128;

trait Loadable<Item, C: Connection> {
    fn iter(self, connection: &mut C) -> anyhow::Result<impl Iterator<Item=QueryResult<Item>>>;
}

pub enum Either<L1, L2> 
{
    L1(L1),
    L2(L2)
}

impl <L1, L2, Item> Iterator for Either<L1,L2>
    where L1: Iterator<Item=Item>, L2: Iterator<Item=Item>
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Either::L1(l1) => l1.next(),
            Either::L2(l2) => l2.next(),
        }
    }
}

impl<L1, L2, Item, C: Connection> Loadable<Item, C> for Either<L1, L2> 
    where L1: Loadable<Item, C>, L2: Loadable<Item, C>
{
    fn iter(self, connection: &mut C) -> anyhow::Result<impl Iterator<Item=QueryResult<Item>>> {
        match self {
            Either::L1(l1) => Ok(Either::L1(l1.iter(connection)?)),
            Either::L2(l2) => Ok(Either::L2(l2.iter(connection)?)),
        }
    }
}

struct LoadableForQuery<RQ>(RQ);

impl<RQ, Item, C: Connection> Loadable<Item, C> for LoadableForQuery<RQ>
    where 
        RQ: 
            RunQueryDsl<C> 
            // + Query<SqlType = Fields> 
            // + QueryFragment<Sqlite>
            // + QueryId
            + LoadQuery<'static, C, Item>
            + 'static,
        Item: 'static
            // Selectable<Sqlite, SelectExpression=DefaultSelection> 
            // + Queryable<Fields, Sqlite>
            // + AsQuery<SqlType = Fields>
            // + 'static
{
    fn iter(self, connection: &mut C) -> anyhow::Result<impl Iterator<Item=QueryResult<Item>>> {
        self.0.load_iter::<Item,DefaultLoadingMode>(connection)
            .context("while loading")
    }
}

fn list_files_query(kind: FileKind) ->  impl Loadable<File, SqliteConnection> {
    let query = 
        files
            .order_by((build_config_id, rel_path))
            .limit(PAGE_SIZE);
    if let FileKind::Built(config_id) = kind {
        return Either::L1(LoadableForQuery(QueryDsl::filter(query, build_config_id.eq(config_id))))
    }
    Either::L2(LoadableForQuery(QueryDsl::filter(query, build_config_id.is_null())))
}

// fn list_files(db: &mut Db, kind: FileKind) -> anyhow::Result<impl Iterator<Item=QueryResult<File>>>{
//     let query = list_files_query(kind);
//     let connection = db.connection();
//     Ok(query.load_iter::<File,DefaultLoadingMode>(connection)?)
// }

// fn sync(source_dir: &Utf8Path)