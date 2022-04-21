use super::worker::NodeType;
use std::fmt;
use std::fmt::Debug;

/// Defines the type of the current node with information specific to each type.
#[derive(Clone, Debug)]
pub enum NodeTypeContainer<M, W> {
    Manager(M),
    Worker(W),
}

impl<M, W> NodeTypeContainer<M, W> {
    pub fn map_manager<F, MOut>(self, f: F) -> NodeTypeContainer<MOut, W>
    where
        F: FnOnce(M) -> MOut,
    {
        match self {
            NodeTypeContainer::Manager(m) => NodeTypeContainer::Manager(f(m)),
            NodeTypeContainer::Worker(w) => NodeTypeContainer::Worker(w),
        }
    }
    pub fn map_manager_ref<F, MOut>(&self, f: F) -> NodeTypeContainer<MOut, &W>
    where
        F: FnOnce(&M) -> MOut,
    {
        match self {
            NodeTypeContainer::Manager(m) => NodeTypeContainer::Manager(f(m)),
            NodeTypeContainer::Worker(w) => NodeTypeContainer::Worker(w),
        }
    }

    pub fn map_worker<F, WOut>(self, f: F) -> NodeTypeContainer<M, WOut>
    where
        F: FnOnce(W) -> WOut,
    {
        match self {
            NodeTypeContainer::Manager(m) => NodeTypeContainer::Manager(m),
            NodeTypeContainer::Worker(w) => NodeTypeContainer::Worker(f(w)),
        }
    }
    pub fn map_worker_ref<F, WOut>(&self, f: F) -> NodeTypeContainer<&M, WOut>
    where
        F: FnOnce(&W) -> WOut,
    {
        match self {
            NodeTypeContainer::Manager(m) => NodeTypeContainer::Manager(m),
            NodeTypeContainer::Worker(w) => NodeTypeContainer::Worker(f(w)),
        }
    }

    /*
    pub fn to_node_type(&self) -> NodeType {
        match self {
            NodeTypeContainer::Manager(_) => NodeType::Manager,
            NodeTypeContainer::Worker(_) => NodeType::Worker,
        }
    }
    */
}

impl<M, W> fmt::Display for NodeTypeContainer<M, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        NodeType::from(self).fmt(f)
    }
}

impl<M, W> From<&NodeTypeContainer<M, W>> for NodeType {
    fn from(src: &NodeTypeContainer<M, W>) -> Self {
        match src {
            NodeTypeContainer::Manager(_) => NodeType::Manager,
            NodeTypeContainer::Worker(_) => NodeType::Worker,
        }
    }
}

impl<M, W> From<NodeTypeContainer<M, W>> for NodeType {
    fn from(src: NodeTypeContainer<M, W>) -> Self {
        match src {
            NodeTypeContainer::Manager(_) => NodeType::Manager,
            NodeTypeContainer::Worker(_) => NodeType::Worker,
        }
    }
}

impl<M: Default, W: Default> From<NodeType> for NodeTypeContainer<M, W> {
    fn from(src: NodeType) -> Self {
        match src {
            NodeType::Manager => NodeTypeContainer::Manager(Default::default()),
            NodeType::Worker => NodeTypeContainer::Worker(Default::default()),
        }
    }
}
