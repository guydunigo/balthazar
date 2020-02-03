use proto::worker::NodeType as ProtoNodeType;

/// Defines the type of the current node,
/// it can also contain information about a known manager.
#[derive(Clone, Debug, Eq)]
pub enum NodeType<T: Clone> {
    Worker(T),
    Manager,
}

impl<T: Clone> NodeType<T> {
    /// Transforms the inner type of the [`NodeType::Worker`].
    pub fn map<NewT: Clone, F>(self, closure: F) -> NodeType<NewT>
    where
        F: FnOnce(T) -> NewT,
    {
        match self {
            NodeType::Manager => NodeType::Manager,
            NodeType::Worker(old) => NodeType::Worker(closure(old)),
        }
    }
}

impl<T: Clone, U: Clone> PartialEq<NodeType<U>> for NodeType<T> {
    fn eq(&self, other: &NodeType<U>) -> bool {
        match (self, other) {
            (NodeType::Manager, NodeType::Manager) | (NodeType::Worker(_), NodeType::Worker(_)) => {
                true
            }
            _ => false,
        }
    }
}

// TODO: not clean
impl From<ProtoNodeType> for NodeType<()> {
    fn from(src: ProtoNodeType) -> Self {
        match src {
            ProtoNodeType::Manager => NodeType::Manager,
            ProtoNodeType::Worker => NodeType::Worker(()),
        }
    }
}
impl<T: Clone> From<NodeType<T>> for ProtoNodeType {
    fn from(src: NodeType<T>) -> Self {
        match src {
            NodeType::Manager => ProtoNodeType::Manager,
            NodeType::Worker(_) => ProtoNodeType::Worker,
        }
    }
}
