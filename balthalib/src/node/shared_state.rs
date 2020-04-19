use super::{Balthazar, LogKind};

use misc::{
    job::{try_bytes_to_address, TaskId},
    shared_state::{PeerId, SharedState, SubTasksState, Task},
};
use proto::{manager as man, worker};

fn check_task_is_known<'a>(
    shared_state: &'a mut impl std::ops::DerefMut<Target = SharedState>,
    task_id: &TaskId,
) -> Result<&'a mut Task, String> {
    shared_state
        .tasks
        .get_mut(task_id)
        .ok_or_else(|| "Task isn't known.".to_string())
}

fn get_substates(task: &mut Task) -> Result<&mut Vec<SubTasksState>, String> {
    task.get_substates()
        .ok_or_else(|| "Task no more incomplete.".to_string())
}

impl Balthazar {
    pub async fn log_shared_state(&self, msg: String) {
        self.spawn_log(LogKind::SharedState, msg).await;
    }
    pub async fn log_res(&self, res: Result<String, String>, name: &str) -> bool {
        let (msg, is_success) = match res {
            Ok(_) => (format!("{}: accepted.", name), true),
            Err(err) => (format!("Error: {}: {}", name, err), false),
        };
        self.log_shared_state(msg).await;
        is_success
    }

    // TODO: check correctness of proof messages, or is it done earlier ?
    // TODO: check we received them...
    pub async fn check_and_apply_proposal(&self, proposal: man::Proposal) {
        let mut shared_state = self.shared_state.write().await;

        let payment_address = match try_bytes_to_address(&proposal.payment_address[..]) {
            Ok(addr) => addr,
            Err(err) => {
                return self
                    .log_shared_state(format!("Could not parse payment address: `{}`.", err))
                    .await;
            }
        };

        let task_id = match TaskId::from_bytes(proposal.task_id) {
            Ok(task_id) => task_id,
            Err(err) => {
                return self
                    .log_shared_state(format!("Could not parse task id in proposal: `{}`.", err))
                    .await;
            }
        };

        // Is the proposal accepted and registered ?
        let (is_accepted_res, name) = match proposal.proposal {
            Some(man::ProposalKind::NewTask(p)) => (
                self.handle_proposal_new_task(&mut shared_state, &task_id, p)
                    .await,
                "ProposeNewTask",
            ),
            Some(man::ProposalKind::Failure(p)) => (
                self.handle_proposal_failure(&mut shared_state, &task_id, p)
                    .await,
                "ProposeFailure",
            ),
            Some(man::ProposalKind::Scheduling(p)) => (
                self.handle_proposal_scheduling(&mut shared_state, &task_id, p)
                    .await,
                "ProposeScheduling",
            ),
            Some(man::ProposalKind::Checked(p)) => (
                self.handle_proposal_checked(&mut shared_state, &task_id, p)
                    .await,
                "ProposeChecked",
            ),
            Some(man::ProposalKind::Completed(p)) => (
                self.handle_proposal_completed(&mut shared_state, &task_id, p)
                    .await,
                "ProposeCompleted",
            ),
            None => {
                return self.log_shared_state("Empty proposal...".to_string()).await;
            }
        };

        let is_accepted = self.log_res(is_accepted_res, name).await;
        if is_accepted {
            if let Some(task) = shared_state.tasks.get_mut(&task_id) {
                task.managers_addresses_mut().push(payment_address);
            } else {
                unreachable!("Still no task, but we aren't supposed to reach here...");
            }
        }
    }

    async fn handle_proposal_new_task(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeNewTask,
    ) -> Result<String, String> {
        if shared_state.tasks.contains_key(task_id) {
            Err("Task already known.".to_string())
        } else {
            shared_state
                .tasks
                .insert(task_id.clone(), Task::new(task_id.clone()));
            Ok("Task registered.".to_string())
        }
    }

    async fn handle_proposal_failure(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeFailure,
    ) -> Result<String, String> {
        Err("Not implemented.".to_string())
        /*
        let mut task = check_task_is_known(shared_state, task_id)?;
        let substates = if let Some(substates) = task.get_substates() {
            substates
        } else {
            return Err("Task no more incomplete.".to_string());
        };

        let max_failures = {
            let chain = self.chain();
            let (job_id, _) = if let Ok(ok) = chain.jobs_get_task(&task_id, true).await {
                ok
            } else {
                return log(format!(
                    "Error, could not find task `{}` in Jobs smart-contract",
                    task_id
                ))
                .await;
            };
            let (_, _, max_failures) =
                if let Ok(ok) = chain.jobs_get_parameters(&job_id, true).await {
                    ok
                } else {
                    return log(format!(
                        "Error, could not find task `{}` in Jobs smart-contract",
                        task_id
                    ))
                    .await;
                };
            max_failures
        };

        // TODO: Option ?
        let (reason, correct_val) = match prop.kind {
            // TODO: check task statuses and new_nb_failures...
            Some(man::FailureKind::Worker(_)) => (TaskErrorKind::Unknown, true),
            Some(man::FailureKind::ManUnavailable(_)) => {
                (
                    // TODO: better error kind ?
                    TaskErrorKind::Unknown,
                    prop.new_nb_failures == task.nb_failures(),
                )
            }
            Some(man::FailureKind::Results(_)) => (
                TaskErrorKind::IncorrectResult,
                prop.new_nb_failures == task.nb_failures() + 1,
            ),
            Some(man::FailureKind::Specs(_)) => (
                TaskErrorKind::IncorrectSpecification,
                prop.new_nb_failures >= max_failures,
            ),
            None => (TaskErrorKind::Unknown, false),
        };

        if correct_val {
            // TODO: store list of failures ?
            task.set_nb_failures(prop.new_nb_failures);
            log("ProposeFailure".to_string()).await;

            if task.nb_failures() >= max_failures {
                task.set_definitely_failed(reason);
                log("Definitely failed".to_string()).await;
            }
            true
        } else {
            log("Error: ProposeFailure: bad new number of failures".to_string()).await;
            false
        }

        Ok(())
            */
    }

    async fn handle_proposal_scheduling(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeScheduling,
    ) -> Result<String, String> {
        Err("Not implemented.".to_string())
    }

    async fn handle_proposal_checked(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeCheckedRunning,
    ) -> Result<String, String> {
        let mut task = check_task_is_known(shared_state, task_id)?;
        let substates = get_substates(&mut task)?;
        let worker = PeerId::from_bytes(proposal.worker)
            .map_err(|_| "Couln't parse worker PeerId.".to_string())?;
        // TODO: check pinging message and sender

        if let Some(substate) = substates
            .iter_mut()
            .find_map(|s| s.as_mut().filter(|a| *a.worker() == worker))
        {
            substate.update_last_check_timestamp();
            Ok("Last checked timestamp updated.".to_string())
        } else {
            Err("Task not assigned to it.".to_string())
        }
    }

    async fn handle_proposal_completed(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeCompleted,
    ) -> Result<String, String> {
        // TODO: check completion signals...
        // TODO: check that selected_result is part of other signals...
        let mut task = check_task_is_known(shared_state, task_id)?;
        if !task.is_incomplete() {
            return Err("Task already no more incomplete.".to_string());
        }

        let substates = get_substates(&mut task)?;
        let (task_id_2, worker, status) = if let Some(man::ManTaskStatus {
            task_id: task_id_2,
            worker,
            status: Some(status),
        }) = proposal.selected_result
        {
            let worker = PeerId::from_bytes(worker)
                .map_err(|_| "Couln't parse worker PeerId.".to_string())?;
            (task_id_2, worker, status)
        } else {
            return Err("Empty selected_result or status.".to_string());
        };

        if &task_id_2[..] != task_id.as_bytes() {
            return Err("ManTaskStatus not corresponding to this task.".to_string());
        }

        // TODO: already checked when receiving it ?
        if substates
            .iter()
            .all(|s| s.as_ref().filter(|a| *a.worker() == worker).is_none())
        {
            return Err("ManTaskStatus worker not assigned to this task.".to_string());
        }

        // TODO: already checked when receiving it ?
        let (task_id_3, result) = if let worker::TaskStatus {
            task_id: task_id_3,
            status_data: Some(worker::StatusData::Result(result)),
        } = status
        {
            (task_id_3, result)
        } else {
            return Err("No status data.".to_string());
        };

        // TODO: already checked when receiving it ?
        if &task_id_3[..] != task_id.as_bytes() {
            return Err("TaskStatus not corresponding to this task.".to_string());
        }

        task.set_completed(result);
        // TODO: store in the BC
        Ok("Result stored...".to_string())
    }
}
