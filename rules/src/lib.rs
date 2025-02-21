use anyhow::Context;
use camino::Utf8Path;
use include_dir::{include_dir, Dir};

static STARLARK: Dir = include_dir!("$CARGO_MANIFEST_DIR/starlark");

pub fn copy_built_in_rules() -> anyhow::Result<()> {
    std::fs::remove_dir_all(store::rules_dir()).context("while removing rules directory")?;

    for file in STARLARK.files() {
        let path = store::rules_dir().join(Utf8Path::from_path(file.path()).unwrap());
        std::fs::create_dir_all(path.parent().unwrap())
            .context("while creating parent directories")?;
        std::fs::write(&path, file.contents()).context("while writing file")?;
    }

    STARLARK
        .extract(store::rules_dir())
        .context("while copying built-in rules")
}
