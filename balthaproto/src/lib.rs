//! This crate contains the base networking protocols to be used by **balthernet**
//! as well as basic structures and functions to handle them.
//!
//! When adding new messages, you can add a from implementation as seen under here.
extern crate bytes;
extern crate futures;
extern crate futures_codec;
extern crate libp2p;
extern crate prost;

pub mod protobuf;

pub mod worker {
    //! Contains the Protobuf messages classes generated by the **prost** crate.
    use super::protobuf::ProtoBufProtocol;

    /// The version number of the worker protocol
    ///
    /// TODO: Use the crate version ?
    pub const PROTOCOL_VERSION: &[u8] = b"/balthazar/worker/0.1.0";

    pub fn new_worker_protocol() -> ProtoBufProtocol<WorkerMsgWrapper> {
        ProtoBufProtocol::new(PROTOCOL_VERSION)
    }

    include!(concat!(env!("OUT_DIR"), "/worker.rs"));

    pub use worker_msg_wrapper::Msg as WorkerMsg;

    impl From<WorkerMsg> for WorkerMsgWrapper {
        fn from(src: WorkerMsg) -> Self {
            WorkerMsgWrapper { msg: Some(src) }
        }
    }

    impl From<NodeTypeRequest> for WorkerMsgWrapper {
        fn from(src: NodeTypeRequest) -> Self {
            WorkerMsg::NodeTypeRequest(src).into()
        }
    }

    impl From<NodeTypeAnswer> for WorkerMsgWrapper {
        fn from(src: NodeTypeAnswer) -> Self {
            WorkerMsg::NodeTypeAnswer(src).into()
        }
    }

    impl From<ManagerRequest> for WorkerMsgWrapper {
        fn from(src: ManagerRequest) -> Self {
            WorkerMsg::ManagerRequest(src).into()
        }
    }

    impl From<ManagerAnswer> for WorkerMsgWrapper {
        fn from(src: ManagerAnswer) -> Self {
            WorkerMsg::ManagerAnswer(src).into()
        }
    }

    impl From<NotMine> for WorkerMsgWrapper {
        fn from(src: NotMine) -> Self {
            WorkerMsg::NotMine(src).into()
        }
    }

    impl From<ManagerBye> for WorkerMsgWrapper {
        fn from(src: ManagerBye) -> Self {
            WorkerMsg::ManagerBye(src).into()
        }
    }

    impl From<ManagerByeAnswer> for WorkerMsgWrapper {
        fn from(src: ManagerByeAnswer) -> Self {
            WorkerMsg::ManagerByeAnswer(src).into()
        }
    }

    impl From<ExecuteTask> for WorkerMsgWrapper {
        fn from(src: ExecuteTask) -> Self {
            WorkerMsg::ExecuteTask(src).into()
        }
    }

    impl From<TaskResult> for WorkerMsgWrapper {
        fn from(src: TaskResult) -> Self {
            WorkerMsg::TaskResult(src).into()
        }
    }
}
