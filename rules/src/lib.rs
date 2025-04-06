use anyhow::Context;
use camino::Utf8Path;
use directories::rules_dir;
use include_dir::{Dir, include_dir};
use std::fs::File;
use std::io::Write;
use tracing::debug;

static STARLARK: Dir = include_dir!("$CARGO_MANIFEST_DIR/starlark");

pub fn copy_built_in_rules() -> anyhow::Result<()> {
    if rules_dir().exists() {
        debug!(
            "Deleting existing copy of internal rules {:?}.",
            rules_dir()
        );
        std::fs::remove_dir_all(rules_dir()).context("while removing rules directory")?;
    }

    debug!("Copying internal rules to {:?}.", rules_dir());
    for file in STARLARK.files() {
        let path = directories::rules_dir().join(Utf8Path::from_path(file.path()).unwrap());
        std::fs::create_dir_all(path.parent().unwrap())
            .context("while creating parent directories")?;
        let mut fs_file = File::create(&path)?;
        fs_file
            .write(file.contents())
            .context("while writing file")?;
        let mut perms = fs_file.metadata()?.permissions();
        perms.set_readonly(true);
        fs_file.set_permissions(perms)?
    }

    STARLARK
        .extract(directories::rules_dir())
        .context("while copying built-in rules")
}
