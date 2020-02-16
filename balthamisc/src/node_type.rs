pub use proto::worker::NodeType;
use std::fmt;
use std::fmt::Debug;

/// Defines the type of the current node with information specific to each type.
#[derive(Clone, Debug)]
pub enum NodeTypeContainer<M, W> {
    Manager(M),
    Worker(W),
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
