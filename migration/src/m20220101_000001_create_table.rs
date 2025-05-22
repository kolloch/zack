use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BuildConfig::Table)
                    .if_not_exists()
                    .col(pk_auto(BuildConfig::Id))
                    .col(string(BuildConfig::Name))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(File::Table)
                    .if_not_exists()
                    .col(pk_auto(File::Id))
                    .col(integer(File::BuildConfigId))
                    .col(string(File::RelPath))
                    .col(blob_null(File::ContentHash))
                    .to_owned(),
            )
            .await?;

        // not supported by sqlite
        // manager
        //     .create_foreign_key(
        //         ForeignKey::create()
        //         .name("FK_build_config")
        //         .from(File::Table, (File::BuildConfigId, BuildConfig::Id))
        //         .to(BuildConfig::Table, (File::BuildConfigId, BuildConfig::Id))
        //         .on_delete(ForeignKeyAction::Cascade)
        //         .on_update(ForeignKeyAction::Cascade)
        //         .to_owned()
        //     ).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(File::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(File::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum BuildConfig {
    Table,
    Id,
    Name,
}

#[derive(DeriveIden)]
enum File {
    Table,
    Id,
    BuildConfigId,
    RelPath,
    ContentHash,
}
