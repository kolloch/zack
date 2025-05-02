use diesel::{connection::DefaultLoadingMode, ExpressionMethods, QueryDsl, QueryResult, RunQueryDsl};

use crate::{db::Db, model::{File, FileKind}};
use crate::schema::files::dsl::*;


const PAGE_SIZE: i64 = 128;

#[diesel::dsl::auto_type(no_type_alias)]
fn list_files_query(kind: FileKind) -> _ {
    let query = 
        files
            .order_by((build_config_id, rel_path))
            .limit(PAGE_SIZE);
    if let FileKind::Built(config_id) = kind {
        return QueryDsl::filter(query, build_config_id.eq(config_id))
    }
    QueryDsl::filter(query, build_config_id.is_null())
}

fn list_files(db: &mut Db, kind: FileKind) -> anyhow::Result<impl Iterator<Item=QueryResult<File>>>{
    let query = list_files_query(kind);
    let connection = db.connection();
    Ok(query.load_iter::<File,DefaultLoadingMode>(connection)?)
}

// fn sync(source_dir: &Utf8Path)