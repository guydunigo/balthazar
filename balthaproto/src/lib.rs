//! This crate contains networking protocols to be used by the **balthernet**.
extern crate bytes;
extern crate futures;
extern crate futures_codec;
extern crate libp2p;
extern crate prost;

pub mod protobuf;

pub mod worker {
    //! Contains the Protobuf messages classes generated by the **prost** crate.

    pub const PROTOCOL_VERSION: &[u8] = b"/balthazar/worker/0.1.0";

    include!(concat!(env!("OUT_DIR"), "/worker.rs"));

    pub use worker_msg_wrapper::Msg as WorkerMsg;

    impl From<WorkerMsg> for WorkerMsgWrapper {
        fn from(src: WorkerMsg) -> Self {
            WorkerMsgWrapper { msg: Some(src) }
        }
    }

    impl From<Task> for WorkerMsgWrapper {
        fn from(src: Task) -> Self {
            WorkerMsg::Task(src).into()
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
}