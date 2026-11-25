// Bounded-concurrency cooperative round-robin async task scheduler.
// Pulls arbitrary-length job queues from `Store<jobs.JobSpec>`, schedules
// concurrent waves of up to 3 live `Task<i32>` workers simultaneously, enforces
// per-job and global tick budgets via explicit `task_request_cancel ->
// task_poll_close -> task_finish_destroy`, proves every `jobs.JobHandle`
// transition (`Queued -> Running -> Completed | Cancelled`) via `match h.state`
// state-protocol refinement, and fans out events to heterogeneous
// `Store<OwnedDyn<dyn jobs.EventSink>>`.
import jobs
import std.io
import std.int

pub struct ScheduleReport {
    pub completed: i32,
    pub cancelled: i32,
    pub total_ticks: i32,
    pub counter_checksum: i32,
    pub audit_metric: i32,
}

fn close_and_destroy_task(t: Managed<Task<i32>>) {
    var owned = give t
    task_request_cancel(&mut owned)
    loop {
        match task_poll_close(&mut owned) {
            std.task.Poll.Pending() => { continue },
            std.task.Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give owned)
}

fn notify_complete(
    sinks: Store<OwnedDyn<dyn jobs.EventSink>>,
    job_id: i32,
    value: i32,
    ticks: i32,
) {
    let n = store_len(sinks)
    var i: i64 = 0
    while i < n {
        let s = store_at_mut(sinks, i)
        let _ = s.on_complete(job_id, value, ticks)
        i += 1
    }
}

fn notify_cancel(
    sinks: Store<OwnedDyn<dyn jobs.EventSink>>,
    job_id: i32,
    ticks: i32,
) {
    let n = store_len(sinks)
    var i: i64 = 0
    while i < n {
        let s = store_at_mut(sinks, i)
        let _ = s.on_cancel(job_id, ticks)
        i += 1
    }
}

fn push_handle(dst: Store<jobs.JobHandle>, h: jobs.JobHandle) -> jobs.SchedulerError!void {
    if !store_reserve(dst, store_len(dst) + 1) {
        return jobs.SchedulerError.Allocation
    }
    match store_push(dst, h) {
        .err(_) => { return jobs.SchedulerError.Allocation },
        .ok(_) => { return () }
    }
}

enum SlotOutcome {
    Pending,
    Completed(i32),
    Cancelled,
}

// Polls one `Running` job handle + task pair. State-protocol contract enforces
// that callers only poll jobs proved `Running`, transitioning them to
// `Running`, `Completed`, or `Cancelled`.
fn step_running_slot(
    task: &mut Task<i32>,
    handle: &mut jobs.JobHandle,
    per_job_budget: i32,
) -> SlotOutcome
    contract {
        requires(state(handle, JobPhase.Running));
        provides(state(
            handle,
            JobPhase.Running,
            JobPhase.Completed,
            JobPhase.Cancelled
        ));
    }
{
    handle.ticks_used += 1
    match task_poll(task) {
        .err(_) => {
            jobs.mark_job_cancelled(handle)
            return SlotOutcome.Cancelled()
        },
        .ok(p) => {
            match p {
                std.task.Poll.Ready(val) => {
                    jobs.mark_job_completed(handle, val)
                    return SlotOutcome.Completed(val)
                },
                std.task.Poll.Pending() => {
                    if per_job_budget > 0 && handle.ticks_used >= per_job_budget {
                        jobs.mark_job_cancelled(handle)
                        return SlotOutcome.Cancelled()
                    }
                    return SlotOutcome.Pending()
                }
            }
        }
    }
}

fn dispatch_slot_step(
    task: &mut Task<i32>,
    handle: &mut jobs.JobHandle,
    sinks: Store<OwnedDyn<dyn jobs.EventSink>>,
    per_job_budget: i32,
    completed: &mut i32,
    cancelled: &mut i32,
) -> bool {
    match handle.state {
        jobs.JobPhase.Running() => {
            match step_running_slot(task, handle, per_job_budget) {
                SlotOutcome.Pending() => { return true },
                SlotOutcome.Completed(val) => {
                    *completed += 1
                    notify_complete(sinks, handle.id, val, handle.ticks_used)
                    return false
                },
                SlotOutcome.Cancelled() => {
                    *cancelled += 1
                    notify_cancel(sinks, handle.id, handle.ticks_used)
                    return false
                }
            }
        },
        jobs.JobPhase.Queued() => { return false },
        jobs.JobPhase.Completed() => { return false },
        jobs.JobPhase.Cancelled() => { return false },
    }
}

fn cancel_if_running(
    handle: &mut jobs.JobHandle,
    sinks: Store<OwnedDyn<dyn jobs.EventSink>>,
    cancelled: &mut i32,
) {
    match handle.state {
        jobs.JobPhase.Running() => {
            jobs.mark_job_cancelled(handle)
            *cancelled += 1
            notify_cancel(sinks, handle.id, handle.ticks_used)
        },
        jobs.JobPhase.Queued() => {
            jobs.mark_job_cancelled(handle)
            *cancelled += 1
            notify_cancel(sinks, handle.id, handle.ticks_used)
        },
        jobs.JobPhase.Completed() => {},
        jobs.JobPhase.Cancelled() => {},
    }
}

fn run_wave1(
    specs: Store<jobs.JobSpec>,
    handles: Store<jobs.JobHandle>,
    sinks: Store<OwnedDyn<dyn jobs.EventSink>>,
    i0: i64,
    per_job_budget: i32,
    global_budget: i32,
    total_ticks: &mut i32,
    completed: &mut i32,
    cancelled: &mut i32,
) -> jobs.SchedulerError!void {
    let sp0 = *store_at(specs, i0)
    match task_create(jobs.cooperative_worker, sp0.id, sp0.steps, sp0.weight) {
        .err(_) => { return jobs.SchedulerError.TaskSpawn },
        .ok(raw0) => {
            var t0 = give raw0
            var h0 = jobs.JobHandle {
                state: jobs.JobPhase.Queued,
                id: sp0.id,
                steps: sp0.steps,
                weight: sp0.weight,
                ticks_used: 0,
                result: 0,
            }
            jobs.mark_job_running(&mut h0)
            var alive0 = true
            while alive0 {
                if global_budget > 0 && *total_ticks >= global_budget {
                    break
                }
                *total_ticks += 1
                alive0 = dispatch_slot_step(
                    &mut t0,
                    &mut h0,
                    sinks,
                    per_job_budget,
                    completed,
                    cancelled,
                )
            }
            cancel_if_running(&mut h0, sinks, cancelled)
            close_and_destroy_task(give t0)
            push_handle(handles, h0)?
            return ()
        }
    }
}

fn run_wave2(
    specs: Store<jobs.JobSpec>,
    handles: Store<jobs.JobHandle>,
    sinks: Store<OwnedDyn<dyn jobs.EventSink>>,
    i0: i64,
    i1: i64,
    per_job_budget: i32,
    global_budget: i32,
    total_ticks: &mut i32,
    completed: &mut i32,
    cancelled: &mut i32,
) -> jobs.SchedulerError!void {
    let sp0 = *store_at(specs, i0)
    let sp1 = *store_at(specs, i1)
    match task_create(jobs.cooperative_worker, sp0.id, sp0.steps, sp0.weight) {
        .err(_) => { return jobs.SchedulerError.TaskSpawn },
        .ok(raw0) => {
            var t0 = give raw0
            match task_create(jobs.cooperative_worker, sp1.id, sp1.steps, sp1.weight) {
                .err(_) => {
                    close_and_destroy_task(give t0)
                    return jobs.SchedulerError.TaskSpawn
                },
                .ok(raw1) => {
                    var t1 = give raw1
                    var h0 = jobs.JobHandle {
                        state: jobs.JobPhase.Queued,
                        id: sp0.id,
                        steps: sp0.steps,
                        weight: sp0.weight,
                        ticks_used: 0,
                        result: 0,
                    }
                    var h1 = jobs.JobHandle {
                        state: jobs.JobPhase.Queued,
                        id: sp1.id,
                        steps: sp1.steps,
                        weight: sp1.weight,
                        ticks_used: 0,
                        result: 0,
                    }
                    jobs.mark_job_running(&mut h0)
                    jobs.mark_job_running(&mut h1)
                    var alive0 = true
                    var alive1 = true
                    while alive0 || alive1 {
                        if global_budget > 0 && *total_ticks >= global_budget {
                            break
                        }
                        *total_ticks += 1
                        if alive0 {
                            alive0 = dispatch_slot_step(
                                &mut t0,
                                &mut h0,
                                sinks,
                                per_job_budget,
                                completed,
                                cancelled,
                            )
                        }
                        if alive1 {
                            alive1 = dispatch_slot_step(
                                &mut t1,
                                &mut h1,
                                sinks,
                                per_job_budget,
                                completed,
                                cancelled,
                            )
                        }
                    }
                    cancel_if_running(&mut h0, sinks, cancelled)
                    cancel_if_running(&mut h1, sinks, cancelled)
                    close_and_destroy_task(give t1)
                    close_and_destroy_task(give t0)
                    push_handle(handles, h0)?
                    push_handle(handles, h1)?
                    return ()
                }
            }
        }
    }
}

fn run_wave3(
    specs: Store<jobs.JobSpec>,
    handles: Store<jobs.JobHandle>,
    sinks: Store<OwnedDyn<dyn jobs.EventSink>>,
    i0: i64,
    i1: i64,
    i2: i64,
    per_job_budget: i32,
    global_budget: i32,
    total_ticks: &mut i32,
    completed: &mut i32,
    cancelled: &mut i32,
) -> jobs.SchedulerError!void {
    let sp0 = *store_at(specs, i0)
    let sp1 = *store_at(specs, i1)
    let sp2 = *store_at(specs, i2)
    match task_create(jobs.cooperative_worker, sp0.id, sp0.steps, sp0.weight) {
        .err(_) => { return jobs.SchedulerError.TaskSpawn },
        .ok(raw0) => {
            var t0 = give raw0
            match task_create(jobs.cooperative_worker, sp1.id, sp1.steps, sp1.weight) {
                .err(_) => {
                    close_and_destroy_task(give t0)
                    return jobs.SchedulerError.TaskSpawn
                },
                .ok(raw1) => {
                    var t1 = give raw1
                    match task_create(jobs.cooperative_worker, sp2.id, sp2.steps, sp2.weight) {
                        .err(_) => {
                            close_and_destroy_task(give t1)
                            close_and_destroy_task(give t0)
                            return jobs.SchedulerError.TaskSpawn
                        },
                        .ok(raw2) => {
                            var t2 = give raw2
                            var h0 = jobs.JobHandle {
                                state: jobs.JobPhase.Queued,
                                id: sp0.id,
                                steps: sp0.steps,
                                weight: sp0.weight,
                                ticks_used: 0,
                                result: 0,
                            }
                            var h1 = jobs.JobHandle {
                                state: jobs.JobPhase.Queued,
                                id: sp1.id,
                                steps: sp1.steps,
                                weight: sp1.weight,
                                ticks_used: 0,
                                result: 0,
                            }
                            var h2 = jobs.JobHandle {
                                state: jobs.JobPhase.Queued,
                                id: sp2.id,
                                steps: sp2.steps,
                                weight: sp2.weight,
                                ticks_used: 0,
                                result: 0,
                            }
                            jobs.mark_job_running(&mut h0)
                            jobs.mark_job_running(&mut h1)
                            jobs.mark_job_running(&mut h2)
                            var alive0 = true
                            var alive1 = true
                            var alive2 = true
                            while alive0 || alive1 || alive2 {
                                if global_budget > 0 && *total_ticks >= global_budget {
                                    break
                                }
                                *total_ticks += 1
                                if alive0 {
                                    alive0 = dispatch_slot_step(
                                        &mut t0,
                                        &mut h0,
                                        sinks,
                                        per_job_budget,
                                        completed,
                                        cancelled,
                                    )
                                }
                                if alive1 {
                                    alive1 = dispatch_slot_step(
                                        &mut t1,
                                        &mut h1,
                                        sinks,
                                        per_job_budget,
                                        completed,
                                        cancelled,
                                    )
                                }
                                if alive2 {
                                    alive2 = dispatch_slot_step(
                                        &mut t2,
                                        &mut h2,
                                        sinks,
                                        per_job_budget,
                                        completed,
                                        cancelled,
                                    )
                                }
                            }
                            cancel_if_running(&mut h0, sinks, cancelled)
                            cancel_if_running(&mut h1, sinks, cancelled)
                            cancel_if_running(&mut h2, sinks, cancelled)
                            close_and_destroy_task(give t2)
                            close_and_destroy_task(give t1)
                            close_and_destroy_task(give t0)
                            push_handle(handles, h0)?
                            push_handle(handles, h1)?
                            push_handle(handles, h2)?
                            return ()
                        }
                    }
                }
            }
        }
    }
}

fn print_i32(v: i32) {
    let dec = int.format_i32(v)
    let _ = io.print(dec.bytes[0..dec.len])
}

fn print_job_summary(h: &jobs.JobHandle) {
    match h.state {
        jobs.JobPhase.Completed() => {
            let _ = io.print("JOB ")
            print_i32(h.id)
            let _ = io.print(" COMPLETED value=")
            print_i32(h.result)
            let _ = io.print(" ticks=")
            print_i32(h.ticks_used)
            let _ = io.print("\n")
        },
        jobs.JobPhase.Cancelled() => {
            let _ = io.print("JOB ")
            print_i32(h.id)
            let _ = io.print(" CANCELLED ticks=")
            print_i32(h.ticks_used)
            let _ = io.print("\n")
        },
        jobs.JobPhase.Queued() => {
            let _ = io.print("JOB ")
            print_i32(h.id)
            let _ = io.print(" QUEUED\n")
        },
        jobs.JobPhase.Running() => {
            let _ = io.print("JOB ")
            print_i32(h.id)
            let _ = io.print(" RUNNING\n")
        }
    }
}

// Runs all `specs` in bounded-concurrency waves of up to 3 round-robin tasks.
// Each job may run at most `per_job_budget` ticks (0 = unlimited), and the
// whole schedule runs at most `global_budget` scheduler rounds (0 = unlimited).
pub fn run_schedule(
    specs: Store<jobs.JobSpec>,
    per_job_budget: i32,
    global_budget: i32,
    verbose: bool,
) -> jobs.SchedulerError!ScheduleReport {
    let total = store_len(specs)
    let handles: Store<jobs.JobHandle> = store_new()
    if !store_reserve(handles, total) {
        return jobs.SchedulerError.Allocation
    }

    let sinks = jobs.build_default_sinks()?
    var completed = 0
    var cancelled = 0
    var total_ticks = 0
    var cursor: i64 = 0

    while cursor < total {
        if global_budget > 0 && total_ticks >= global_budget {
            let sp = *store_at(specs, cursor)
            var h = jobs.JobHandle {
                state: jobs.JobPhase.Queued,
                id: sp.id,
                steps: sp.steps,
                weight: sp.weight,
                ticks_used: 0,
                result: 0,
            }
            jobs.mark_job_cancelled(&mut h)
            cancelled += 1
            notify_cancel(sinks, h.id, 0)
            push_handle(handles, h)?
            cursor += 1
            continue
        }
        let remaining = total - cursor
        if remaining >= 3 {
            run_wave3(
                specs,
                handles,
                sinks,
                cursor,
                cursor + 1,
                cursor + 2,
                per_job_budget,
                global_budget,
                &mut total_ticks,
                &mut completed,
                &mut cancelled,
            )?
            cursor += 3
        } else {
            if remaining == 2 {
                run_wave2(
                    specs,
                    handles,
                    sinks,
                    cursor,
                    cursor + 1,
                    per_job_budget,
                    global_budget,
                    &mut total_ticks,
                    &mut completed,
                    &mut cancelled,
                )?
                cursor += 2
            } else {
                run_wave1(
                    specs,
                    handles,
                    sinks,
                    cursor,
                    per_job_budget,
                    global_budget,
                    &mut total_ticks,
                    &mut completed,
                    &mut cancelled,
                )?
                cursor += 1
            }
        }
    }

    if verbose {
        var pi: i64 = 0
        while pi < store_len(handles) {
            print_job_summary(store_at(handles, pi))
            pi += 1
        }
    }

    let counter_checksum = store_at(sinks, 0).summary_metric()
    let audit_metric = store_at(sinks, 1).summary_metric()

    return ScheduleReport {
        completed: completed,
        cancelled: cancelled,
        total_ticks: total_ticks,
        counter_checksum: counter_checksum,
        audit_metric: audit_metric,
    }
}

// Stress-tests repeated spawn -> partial poll -> cancel -> close -> destroy cycles
// to verify deterministic teardown and zero leaked task frames or buffers.
pub fn run_cancel_stress(rounds: i32) -> jobs.SchedulerError!i32 {
    var cancelled = 0
    var r = 0
    while r < rounds {
        match task_create(jobs.cooperative_worker, r + 1, 20, 3) {
            .err(_) => { return jobs.SchedulerError.TaskSpawn },
            .ok(task) => {
                var t = give task
                // Poll once so the task actually allocates its scratch buffer and suspends mid-loop.
                match task_poll(&mut t) {
                    .err(_) => {},
                    .ok(p) => {
                        match p {
                            std.task.Poll.Pending() => {},
                            std.task.Poll.Ready(_) => {}
                        }
                    }
                }
                close_and_destroy_task(give t)
                cancelled += 1
            }
        }
        r += 1
    }
    return cancelled
}
