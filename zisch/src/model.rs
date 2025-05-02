use std::fmt::Display;

use camino::Utf8PathBuf;
use diesel::{backend::Backend, deserialize::{self, FromSql, FromSqlRow}, expression::AsExpression, prelude::{Insertable, Queryable}, sql_types, Selectable};
use diesel::serialize::{self, ToSql, Output};
use std::io::Write;

#[derive(AsExpression, FromSqlRow, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[diesel(sql_type = sql_types::Integer)]
pub struct BuildConfigId(i32);

impl BuildConfigId {
    pub const DEFAULT_CONFIG_ID: BuildConfigId = BuildConfigId(1);
}

#[derive(Queryable, Selectable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = crate::schema::build_configs)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct BuildConfig {
    pub id: BuildConfigId,
    pub name: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::build_configs)]
pub struct NewBuildConfig<'a> {
    pub name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    Source,
    Built(BuildConfigId),
}


#[derive(AsExpression, FromSqlRow, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[diesel(sql_type = sql_types::Integer)]
pub struct FileId(i32);

#[derive(Queryable, Selectable, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[diesel(table_name = crate::schema::files)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct File {
    pub id: FileId,
    build_config_id: Option<BuildConfigId>,
    pub rel_path: DbPathBuf,
    pub content_hash: Hash,
}

#[derive(AsExpression, FromSqlRow, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[diesel(sql_type = sql_types::Text)]
pub struct DbPathBuf(Utf8PathBuf);

#[derive(AsExpression, FromSqlRow, Debug, Clone, PartialEq, Eq, Hash)]
#[diesel(sql_type = sql_types::Binary)]
pub struct Hash {
    internal: blake3::Hash
}

impl<DB> FromSql<sql_types::Integer, DB> for BuildConfigId
where
    DB: Backend,
    i32: FromSql<sql_types::Integer, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> deserialize::Result<Self> {
        let value = i32::from_sql(bytes)?;
        Ok(BuildConfigId(value))
    }
}

impl<DB> ToSql<sql_types::Integer, DB> for BuildConfigId
where
    DB: Backend,
    i32: ToSql<sql_types::Integer, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl<DB> ToSql<sql_types::Integer, DB> for FileId
where
    DB: Backend,
    i32: ToSql<sql_types::Integer, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl<DB> ToSql<sql_types::Text, DB> for DbPathBuf
where
    DB: Backend,
    String: ToSql<sql_types::Text, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.as_str().to_sql(out)
    }
}

impl<DB> ToSql<sql_types::Binary, DB> for Hash
where
    DB: Backend,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        out.write_all(self.internal.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}

impl<DB> FromSql<sql_types::Integer, DB> for FileId
where
    DB: Backend,
    i32: FromSql<sql_types::Integer, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> deserialize::Result<Self> {
        let value = i32::from_sql(bytes)?;
        Ok(FileId(value))
    }
}

impl<DB> FromSql<sql_types::Text, DB> for DbPathBuf 
where
    DB: Backend,
    String: FromSql<sql_types::Text, DB>,
{
    fn from_sql(bytes: <DB as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let string = String::from_sql(bytes)?;
        Ok(DbPathBuf(string.into()))
    }
}

impl<DB> FromSql<sql_types::Binary, DB> for Hash 
where
    DB: Backend,
    Vec<u8>: FromSql<sql_types::Binary, DB>,
{
    fn from_sql(bytes: <DB as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let bytes_vec = Vec::<u8>::from_sql(bytes)?;
        if bytes_vec.len() != blake3::OUT_LEN {
            return Err(format!("Invalid hash length: expected {} bytes, got {}", 
            blake3::OUT_LEN, bytes_vec.len()).into());
        }
        
        let mut hash_bytes = [0u8; blake3::OUT_LEN];
        hash_bytes.copy_from_slice(&bytes_vec);
        Ok(Hash {
            internal: blake3::Hash::from_bytes(hash_bytes)
        })
    }
}

impl Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.internal)
    }
}

impl Ord for Hash {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.internal.as_bytes().cmp(other.internal.as_bytes())
    }
}
impl PartialOrd for Hash {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
