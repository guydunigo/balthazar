use super::{try_convert_task_error_kind, Error};
use misc::job::{DefaultHash, JobId, TaskId};
use proto::manager::TaskDefiniteErrorKind;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use web3::{
    ethabi,
    types::{self, Address, Log},
};

/// Parsed events received from the Jobs smart-contract.
#[derive(Debug, Clone)]
pub enum JobsEvent {
    // CounterHasNewValue(u128),
    JobNew {
        sender: Address,
        nonce: u128,
    },
    TaskPending {
        task_id: TaskId,
    },
    TaskCompleted {
        task_id: TaskId,
        result: Vec<u8>,
    },
    TaskDefinetelyFailed {
        task_id: TaskId,
        reason: TaskDefiniteErrorKind,
    },
    PendingMoneyChanged {
        account: Address,
        new_val: u128,
    },
    JobCompleted {
        job_id: JobId,
    },
}

impl fmt::Display for JobsEvent {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsEvent::JobNew { sender, nonce } => write!(
                fmt,
                "JobNew {{ sender: {}, nonce: {}, job_id: {} }}",
                sender,
                nonce,
                JobId::job_id(sender, *nonce)
            ),
            JobsEvent::TaskPending { task_id } => {
                write!(fmt, "TaskPending {{ task_id: {} }}", task_id)
            }
            JobsEvent::TaskCompleted { task_id, result } => write!(
                fmt,
                "TaskCompleted {{ task_id: {}, result: {} }}",
                task_id,
                String::from_utf8_lossy(&result[..])
            ),
            JobsEvent::JobCompleted { job_id } => {
                write!(fmt, "JobCompleted {{ job_id: {} }}", job_id)
            }
            _ => write!(fmt, "{:?}", self),
        }
    }
}

impl TryFrom<(&ethabi::Contract, Log)> for JobsEvent {
    type Error = Error;

    // TODO: ugly ?
    // TODO: use ethabi and all functions to auto parse ?
    /// We assume that [`Log::data`]] is just all the arguments of the events as `[u8; 256]`
    /// concatenated together.
    fn try_from((contract, log): (&ethabi::Contract, Log)) -> Result<Self, Self::Error> {
        for t in log.topics.iter() {
            if let Some(evt) = contract.events().find(|e| e.signature() == *t) {
                match JobsEventKind::try_from(&evt.name[..])? {
                    /*
                    JobsEventKind::CounterHasNewValue => {
                        let len = log.data.0.len();
                        if len == 32 {
                            let counter =
                                types::U128::from_big_endian(&log.data.0[(len - 16)..len])
                                    .as_u128();
                            return Ok(JobsEvent::CounterHasNewValue(counter));
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected: 32,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    */
                    JobsEventKind::JobNew => {
                        let len = log.data.0.len();
                        let expected = 32 * 2;
                        if len == expected {
                            let sender =
                                Address::from_slice(&log.data.0[(32 - Address::len_bytes())..32]);
                            let nonce =
                                types::U128::from_big_endian(&log.data.0[(64 - 16)..64]).as_u128();
                            return Ok(JobsEvent::JobNew { sender, nonce });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::TaskPending => {
                        let len = log.data.0.len();
                        let expected = DefaultHash::SIZE;
                        if len == expected {
                            let task_id = (&log.data.0[..]).try_into()?;
                            return Ok(JobsEvent::TaskPending { task_id });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::TaskCompleted => {
                        let len = log.data.0.len();
                        let expected = DefaultHash::SIZE;
                        if len > expected {
                            let task_id = (&log.data.0[..DefaultHash::SIZE]).try_into()?;
                            return Ok(JobsEvent::TaskCompleted {
                                task_id,
                                // + 32 because there seems to be an extra 32 bytes
                                // between the two fields.
                                result: Vec::from(&log.data.0[DefaultHash::SIZE + 32..]),
                            });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::TaskDefinetelyFailed => {
                        let len = log.data.0.len();
                        let expected = DefaultHash::SIZE + 32;
                        if len == expected {
                            let task_id = (&log.data.0[..DefaultHash::SIZE]).try_into()?;
                            let reason_nb = types::U64::from_big_endian(
                                &log.data.0[DefaultHash::SIZE + 24..DefaultHash::SIZE + 32],
                            )
                            .as_u64();
                            return Ok(JobsEvent::TaskDefinetelyFailed {
                                task_id,
                                reason: try_convert_task_error_kind(reason_nb)
                                    .ok_or(Error::TaskErrorKindParse(reason_nb))?,
                            });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::PendingMoneyChanged => {
                        let len = log.data.0.len();
                        let expected = 32 * 2;
                        if len == expected {
                            let account =
                                Address::from_slice(&log.data.0[(32 - Address::len_bytes())..32]);
                            // TODO: upper_u128 ?
                            let new_val =
                                types::U256::from_big_endian(&log.data.0[32..64]).low_u128();
                            return Ok(JobsEvent::PendingMoneyChanged { account, new_val });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::JobCompleted => {
                        let len = log.data.0.len();
                        let expected = DefaultHash::SIZE;
                        if len == expected {
                            let job_id = (&log.data.0[..]).try_into()?;
                            return Ok(JobsEvent::JobCompleted { job_id });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                }
            }
        }

        Err(Error::CouldntParseJobsEventFromLog(Box::new(log)))
    }
}

/// Types of events which can be listened to.
#[derive(Debug, Clone, Copy)]
pub enum JobsEventKind {
    // CounterHasNewValue,
    JobNew,
    TaskPending,
    TaskCompleted,
    TaskDefinetelyFailed,
    PendingMoneyChanged,
    JobCompleted,
}

impl std::convert::TryFrom<&str> for JobsEventKind {
    type Error = Error;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        match src {
            // "CounterHasNewValue" => Ok(JobsEventKind::CounterHasNewValue),
            "JobNew" => Ok(JobsEventKind::JobNew),
            "TaskPending" => Ok(JobsEventKind::TaskPending),
            "TaskCompleted" => Ok(JobsEventKind::TaskCompleted),
            "TaskDefinetelyFailed" => Ok(JobsEventKind::TaskDefinetelyFailed),
            "PendingMoneyChanged" => Ok(JobsEventKind::PendingMoneyChanged),
            "JobCompleted" => Ok(JobsEventKind::JobCompleted),
            _ => Err(Error::CouldntParseJobsEventName(String::from(src))),
        }
    }
}

impl std::str::FromStr for JobsEventKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl fmt::Display for JobsEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
