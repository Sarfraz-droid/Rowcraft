use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{Dialect, Error, Result};

/// Lookup table of available dialects, keyed by `Dialect::id()`.
#[derive(Default)]
pub struct Registry {
    dialects: BTreeMap<&'static str, Arc<dyn Dialect>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.dialects.keys()).finish()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, dialect: impl Dialect + 'static) {
        self.dialects.insert(dialect.id().0, Arc::new(dialect));
    }

    pub fn get(&self, id: &str) -> Result<Arc<dyn Dialect>> {
        self.dialects.get(id).cloned().ok_or_else(|| Error::UnknownDialect(id.to_owned()))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Dialect>> {
        self.dialects.values()
    }

    pub fn len(&self) -> usize {
        self.dialects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dialects.is_empty()
    }
}
