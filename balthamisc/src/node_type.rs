use proto::worker::NodeType as ProtoNodeType;
use std::fmt;

/// Defines the type of the current node,
/// it can also contain information about a known manager.
///
/// TODO: Useful? Why not use directly the Message type?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    Manager,
    Worker,
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeType::Manager => write!(f, "Manager"),
            NodeType::Worker => write!(f, "Worker"),
        }
    }
}

impl Default for NodeType {
    /// By default the node is a [`Manager`](`NodeType::Manager`)
    fn default() -> Self {
        // TODO: really ?
        NodeType::Manager
    }
}

impl From<ProtoNodeType> for NodeType {
    fn from(src: ProtoNodeType) -> Self {
        match src {
            ProtoNodeType::Manager => NodeType::Manager,
            ProtoNodeType::Worker => NodeType::Worker,
        }
    }
}

impl From<NodeType> for ProtoNodeType {
    fn from(src: NodeType) -> Self {
        match src {
            NodeType::Manager => ProtoNodeType::Manager,
            NodeType::Worker => ProtoNodeType::Worker,
        }
    }
}
