use super::*;

/// This is for the communication between upper layers (not network).
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Hash)]
pub enum Message {
    Hello(String),
    Idle(usize),
    RequestJob(JobId),
    UnknownJobId(JobId),
    UnknownTaskId(JobId, TaskId),
    Job(PeerId, JobId, Vec<u8>), // TODO: more a signature than JobId
    JobRegisteredAt(JobId),
    Task(JobId, TaskId, Arguments),
    // TODO: or tasks?
    ReturnValue(JobId, TaskId, Result<Arguments, ()>), // TODO: proper error
    // External(E) // TODO: generic type
    NoJob,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Message::*;

        match self {
            Job(pid, jid, bytecode) => {
                write!(f, "Job #{} from `{}` of {} bytes", jid, pid, bytecode.len())
            }
            Task(jid, tid, _) => write!(f, "Task #{} for Job #{}", tid, jid),
            ReturnValue(jid, tid, _) => write!(f, "Result for Task #{} for Job #{}", tid, jid),
            _ => {
                let debug = format!("{:?}", self);
                let len = if debug.len() < 20 { debug.len() } else { 10 };
                write!(f, "{}", &debug[..len])
            }
        }
    }
}
