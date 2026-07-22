use std::{collections::HashMap, sync::Arc};

pub struct Path {
    inner: String,
}

#[derive(Default)]
pub struct Router {
    routes: Arc<HashMap<Path, ()>>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }
}
