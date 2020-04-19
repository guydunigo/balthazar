// TODO: review all in respect to specs and all...
use super::{Balthazar, LogKind};

use misc::{
    job::{try_bytes_to_address, Address, TaskId},
    shared_state::{Assigned, PeerId, SharedState, SubTasksState, Task, TaskCompleteness},
};
use proto::{manager as man, worker, worker::TaskErrorKind};

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
            Ok(ok) => (format!("{}: accepted : {}", name, ok), true),
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
                self.handle_proposal_failure(&mut shared_state, &task_id, p, payment_address)
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
                self.handle_proposal_completed(&mut shared_state, &task_id, p, payment_address)
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
                if task.is_incomplete() {
                    task.managers_addresses_mut().push(payment_address);
                }
            } else {
                unreachable!("Still no task, but we aren't supposed to reach here...");
            }
        }
    }

    async fn handle_proposal_new_task(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        _proposal: man::ProposeNewTask,
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

    // TODO: Check all things sub-messages.
    async fn handle_proposal_failure(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeFailure,
        payment_address: Address,
    ) -> Result<String, String> {
        let mut task = check_task_is_known(shared_state, task_id)?;
        // TODO: empty substates...
        let substates = get_substates(&mut task)?;

        let max_failures = {
            let chain = self.chain();
            let (job_id, _) = chain.jobs_get_task(&task_id, true).await.map_err(|err| {
                format!(
                    "Problem fetchin task `{}` from Jobs smart-contract: {}",
                    task_id, err
                )
            })?;
            let (_, _, max_failures) =
                chain
                    .jobs_get_parameters(&job_id, true)
                    .await
                    .map_err(|err| {
                        format!(
                            "Problem fetching max_failures for job `{}` in Jobs smart-contract: {}",
                            job_id, err
                        )
                    })?;
            max_failures
        };

        // TODO: Option ?
        let (reason, correct_val) = match proposal.kind {
            // TODO: check task statuses and new_nb_failures...
            Some(man::FailureKind::Worker(man::ProposeFailureWorker {
                original_message:
                    Some(man::ManTaskStatus {
                        // TODO: got lazy in testing all parameters, but at the same time it was maybe
                        // already checked right beforehand...
                        status:
                            Some(worker::TaskStatus {
                                status_data: Some(worker::StatusData::Error(err_i32)),
                                ..
                            }),
                        ..
                    }),
                ..
            })) => {
                let error = worker::TaskErrorKind::from_i32(err_i32)
                    .ok_or_else(|| "Couldn't parse error type.".to_string())?;

                use TaskErrorKind::*;

                match error {
                    Aborted | Unknown => (error, proposal.new_nb_failures == task.nb_failures()),
                    TimedOut | Download | Runtime => {
                        (error, proposal.new_nb_failures == task.nb_failures() + 1)
                    }
                    _ => {
                        return Err("Incorrect error kind in TaskStatus.".to_string());
                    }
                }
            }
            Some(man::FailureKind::Worker(_)) => {
                return Err("Invalid or missing original_message.".to_string());
            }
            Some(man::FailureKind::ManUnavailable(man::ProposeFailureManagerUnavailable {
                unanswered_ping:
                    Some(man::PingManagerForTask {
                        task_id: task_id_2,
                        worker,
                        ..
                    }),
            })) => {
                if &task_id_2[..] != task_id.as_bytes() {
                    return Err("Ping for wrong task.".to_string());
                }

                if let Some(substate) = substates.iter_mut().find(|s| {
                    s.as_ref()
                        .filter(|a| a.worker().as_bytes() == &worker[..])
                        .is_some()
                }) {
                    substate.take();
                } else {
                    return Err("Worker not assigned to it.".to_string());
                }

                (
                    // TODO: better error kind ?
                    TaskErrorKind::Unknown,
                    proposal.new_nb_failures == task.nb_failures(),
                )
            }
            Some(man::FailureKind::ManUnavailable(_)) => {
                return Err("Missing unanswered_ping.".to_string());
            }
            Some(man::FailureKind::Results(_)) => {
                substates.iter_mut().for_each(|s| {
                    s.take();
                });
                (
                    TaskErrorKind::IncorrectResult,
                    proposal.new_nb_failures == task.nb_failures() + 1,
                )
            }
            Some(man::FailureKind::Specs(_)) => (
                TaskErrorKind::IncorrectSpecification,
                proposal.new_nb_failures >= max_failures,
            ),
            None => {
                return Err("Missing proposal failure kind.".to_string());
            }
        };

        if !correct_val {
            return Err("Error: ProposeFailure: bad new number of failures".to_string());
        }

        // TODO: store list of failures rather than only last one?
        task.set_nb_failures(proposal.new_nb_failures);

        // Definetely failed if too many failures...
        if task.nb_failures() >= max_failures {
            task.managers_addresses_mut().push(payment_address);
            task.set_definitely_failed(reason);

            let chain = self.chain();
            chain
                .jobs_set_managers(task_id, task.managers_addresses())
                .await
                .map_err(|err| format!("Problem setting managers in the Jobs SC: {}", err))?;
            chain
                .jobs_set_definitely_failed(task_id, reason)
                .await
                .map_err(|err| {
                    format!(
                        "Problem setting task as definitely failed in the Jobs SC: {}",
                        err
                    )
                })?;
            Ok("Definitely failed, set as such in the Jobs SC.".to_string())
        } else {
            Ok("Failed".to_string())
        }
    }

    // TODO: check offers and all...
    async fn handle_proposal_scheduling(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        mut proposal: man::ProposeScheduling,
    ) -> Result<String, String> {
        let mut task = check_task_is_known(shared_state, task_id)?;
        let substates = get_substates(&mut task)?;

        let mut offers_filtered = Vec::new();
        for o in proposal.selected_offers.drain(..) {
            if &o.task_id[..] != task_id.as_bytes() {
                return Err("Unmatching task id in offer.".to_string());
            }

            let payment_address = try_bytes_to_address(&o.payment_address[..])
                .map_err(|_| "Can't parse payment_address in offer.".to_string())?;
            // TODO: check also prices values...

            let worker = PeerId::from_bytes(o.worker)
                .map_err(|_| "Couln't parse worker PeerId.".to_string())?;
            let workers_manager = PeerId::from_bytes(o.workers_manager)
                .map_err(|_| "Couln't parse worker PeerId.".to_string())?;

            offers_filtered.push(Assigned::new(
                worker,
                workers_manager,
                payment_address,
                o.worker_price,
                o.network_price,
            ));
        }

        // TODO: do something else than map ?
        #[allow(clippy::suspicious_map)]
        let nb_assigned = substates
            .iter_mut()
            .filter(|o| o.is_none())
            .zip(offers_filtered.drain(..))
            .map(|(unassigned, selected)| {
                unassigned.replace(selected);
            })
            .count();
        Ok(format!("Stored {} tasks.", nb_assigned))
    }

    // TODO: check pinging message and sender
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

        if let Some(substate) = substates
            .iter_mut()
            .find_map(|s| s.as_mut().filter(|a| *a.worker() == worker))
        {
            substate.update_last_check_timestamp();
            Ok("Last checked timestamp updated.".to_string())
        } else {
            Err("Worker not assigned to it.".to_string())
        }
    }

    // TODO: check completion signals...
    // TODO: check that selected_result is part of other signals...
    async fn handle_proposal_completed(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeCompleted,
        payment_address: Address,
    ) -> Result<String, String> {
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
            return Err("No status data or incorrect one.".to_string());
        };

        // TODO: already checked when receiving it ?
        if &task_id_3[..] != task_id.as_bytes() {
            return Err("TaskStatus not corresponding to this task.".to_string());
        }

        task.managers_addresses_mut().push(payment_address);
        task.set_completed(result);

        let (result, payment_info) = if let TaskCompleteness::Completed {
            workers_payment_info,
            result,
            ..
        } = task.completeness()
        {
            (&result[..], &workers_payment_info[..])
        } else {
            unreachable!("just assigned");
        };

        self.chain()
            .jobs_set_managers(&task_id, task.managers_addresses())
            .await
            .map_err(|err| format!("Couldn't set managers on the Jobs SC: {}", err))?;
        self.chain()
            .jobs_set_completed(&task_id, result, payment_info)
            .await
            .map_err(|err| format!("Couldn't set completed on the Jobs SC: {}", err))?;
        Ok("Result stored...".to_string())
    }
}
