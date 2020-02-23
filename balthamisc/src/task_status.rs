pub use proto::worker::TaskErrorKind;
use proto::worker::{task_status::StatusData as ProtoTaskStatus, Null};
use std::fmt;

/// Defines the status of a given task
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Started(i64),
    Error(TaskErrorKind),
    Completed(Vec<u8>),
    Unknown,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "Pending"),
            // TODO: parse to real date
            TaskStatus::Started(timestamp) => write!(f, "Started on timestamp {}", timestamp),
            TaskStatus::Error(TaskErrorKind::Timedout) => write!(f, "Error: Task has timed out."),
            TaskStatus::Error(TaskErrorKind::Download) => {
                write!(f, "Error: Couln't download program.")
            }
            TaskStatus::Error(TaskErrorKind::Running) => write!(f, "Error during program runtime."),
            TaskStatus::Error(TaskErrorKind::Aborted) => write!(f, "Program was aborted."),
            TaskStatus::Error(TaskErrorKind::Unknown) => {
                write!(f, "An error occured but no description was provided.")
            }
            // TODO: better display of result ?
            TaskStatus::Completed(_) => write!(f, "Completed"),
            TaskStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

impl From<ProtoTaskStatus> for TaskStatus {
    fn from(src: ProtoTaskStatus) -> Self {
        match src {
            ProtoTaskStatus::Pending(_) => TaskStatus::Pending,
            ProtoTaskStatus::StartTime(timestamp) => TaskStatus::Started(timestamp),
            ProtoTaskStatus::Error(err) => TaskStatus::Error(
                TaskErrorKind::from_i32(err).unwrap_or_else(|| TaskErrorKind::Unknown),
            ),
            ProtoTaskStatus::Result(res) => TaskStatus::Completed(res),
            ProtoTaskStatus::Unknown(_) => TaskStatus::Unknown,
        }
    }
}

impl From<TaskStatus> for ProtoTaskStatus {
    fn from(src: TaskStatus) -> Self {
        match src {
            TaskStatus::Pending => ProtoTaskStatus::Pending(Null {}),
            TaskStatus::Started(timestamp) => ProtoTaskStatus::StartTime(timestamp),
            TaskStatus::Error(err) => ProtoTaskStatus::Error(err.into()),
            TaskStatus::Completed(res) => ProtoTaskStatus::Result(res),
            TaskStatus::Unknown => ProtoTaskStatus::Unknown(Null {}),
        }
    }
}

impl From<Option<ProtoTaskStatus>> for TaskStatus {
    fn from(src: Option<ProtoTaskStatus>) -> Self {
        if let Some(status) = src {
            status.into()
        } else {
            TaskStatus::Unknown
        }
    }
}

impl From<TaskStatus> for Option<ProtoTaskStatus> {
    fn from(src: TaskStatus) -> Self {
        Some(src.into())
    }
}