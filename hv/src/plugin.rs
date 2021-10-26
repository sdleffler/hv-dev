use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::Mutex,
};

use petgraph::graphmap::DiGraphMap;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin dependency graph is cyclic around node {_0}")]
    CyclicDependency(&'static str),

    #[error("an error occurred while initializing plugin {_0}")]
    InitError(&'static str, #[source] anyhow::Error),
}

pub struct PluginDependencies<T: 'static> {
    ids: HashSet<TypeId>,
    _phantom: PhantomData<T>,
}

impl<T: 'static> PluginDependencies<T> {
    fn new() -> Self {
        Self {
            ids: HashSet::new(),
            _phantom: PhantomData,
        }
    }

    pub fn add<U: Plugin<T>>(&mut self) -> &mut Self {
        self.ids.insert(TypeId::of::<U>());
        self
    }
}

pub trait Plugin<T: 'static>: 'static {
    /// A human-readable string name for the plugin. Defaults to `std::any::type_name::<Self>()`.
    fn plugin_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn dependencies(&self, dependencies: &mut PluginDependencies<T>) {
        let _ = dependencies;
    }

    fn init(&self, context: &mut T) -> Result<(), anyhow::Error>;
}

impl<T> Default for RegistryInner<T> {
    fn default() -> Self {
        Self {
            plugins: HashMap::new(),
            graph: DiGraphMap::new(),
        }
    }
}

struct RegistryInner<T: 'static> {
    plugins: HashMap<TypeId, Box<dyn Plugin<T>>>,
    graph: DiGraphMap<TypeId, ()>,
}

impl<T: 'static> RegistryInner<T> {
    fn insert<P: Plugin<T>>(&mut self, plugin: P) {
        let type_id = TypeId::of::<P>();
        self.graph.add_node(type_id);

        let mut dependencies = PluginDependencies::new();
        plugin.dependencies(&mut dependencies);
        self.plugins.insert(type_id, Box::new(plugin));

        for dep in dependencies.ids {
            self.graph.add_edge(type_id, dep, ());
        }
    }

    fn run(&mut self, context: &mut T) -> Result<(), PluginError> {
        let nodes = petgraph::algo::toposort(&self.graph, None).map_err(|cycle| {
            PluginError::CyclicDependency(self.plugins[&cycle.node_id()].plugin_name())
        })?;

        for node in nodes {
            self.plugins[&node].init(context).map_err(|anyhow| {
                PluginError::InitError(self.plugins[&node].plugin_name(), anyhow)
            })?;
        }

        Ok(())
    }
}

pub struct PluginRegistry<T: 'static> {
    inner: Mutex<RegistryInner<T>>,
}

impl<T: 'static> Default for PluginRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> PluginRegistry<T> {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RegistryInner::default()),
        }
    }

    pub fn insert(&self, plugin: impl Plugin<T>) {
        self.inner.lock().unwrap().insert(plugin);
    }

    pub fn run(&self, context: &mut T) -> Result<(), PluginError> {
        self.inner.lock().unwrap().run(context)
    }
}
