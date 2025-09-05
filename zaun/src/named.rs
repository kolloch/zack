use std::collections::HashSet;

use derive_more::{Deref, DerefMut, Display};

#[derive(Display, Debug, Clone, Deref, DerefMut)]
#[display("{name}")]
pub struct Named<T> {
    name: String,
    #[deref]
    #[deref_mut]
    value: T,
}

impl<T> Named<T> {
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Clone, Default)]
pub struct Namer {
    used_names: HashSet<String>,
}

impl Namer {
    pub fn named<T>(&mut self, base_name: &str, value: T) -> Named<T> {
        let name = self.unique_name(base_name);
        Named { name, value }
    }

    fn unique_name(&mut self, base: &str) -> String {
        let mut name = base.to_string();
        let mut counter = 2;
        while self.used_names.contains(&name) {
            name = format!("{base}_{counter}");
            counter += 1;
        }
        self.used_names.insert(name.clone());
        name
    }
}
