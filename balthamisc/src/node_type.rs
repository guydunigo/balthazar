use proto::worker::NodeType as ProtoNodeType;

/// Defines the type of the current node,
/// it can also contain information about a known manager.
///
/// TODO: Useful? Why not use directly the Message type?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    Worker,
    Manager,
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
