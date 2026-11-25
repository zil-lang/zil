// Job specifications, protocol-tracked job states, heterogeneous event sinks
// (`trait EventSink` + `OwnedDyn<dyn EventSink>`), and cooperative suspending workers.
import std.int

pub enum SchedulerError {
    Allocation,
    TaskSpawn,
    Io,
    InvalidManifest,
}

pub enum JobPhase {
    Queued,
    Running,
    Completed,
    Cancelled,
}

pub struct JobHandle {
    pub state: JobPhase,
    pub id: i32,
    pub steps: i32,
    pub weight: i32,
    pub ticks_used: i32,
    pub result: i32,
}

pub fn make_job_handle(id: i32, steps: i32, weight: i32) -> JobHandle {
    return JobHandle {
        state: JobPhase.Queued,
        id: id,
        steps: steps,
        weight: weight,
        ticks_used: 0,
        result: 0,
    }
}

pub fn mark_job_running(j: &mut JobHandle)
    contract {
        requires(state(j, JobPhase.Queued));
        provides(state(j, JobPhase.Running));
        effects(invalidates(j));
    }
{
    j.state = JobPhase.Running
}

pub fn mark_job_completed(j: &mut JobHandle, result: i32)
    contract {
        requires(state(j, JobPhase.Running));
        provides(state(j, JobPhase.Completed));
        effects(invalidates(j));
    }
{
    j.result = result
    j.state = JobPhase.Completed
}

pub fn mark_job_cancelled(j: &mut JobHandle)
    contract {
        requires(state(j, JobPhase.Queued, JobPhase.Running));
        provides(state(j, JobPhase.Cancelled));
        effects(invalidates(j));
    }
{
    j.state = JobPhase.Cancelled
}

pub fn job_is_active(j: &JobHandle) -> bool
    contract { effects(invalidates()); }
{
    match j.state {
        JobPhase.Running() => { return true },
        JobPhase.Queued() => { return false },
        JobPhase.Completed() => { return false },
        JobPhase.Cancelled() => { return false },
    }
}

pub struct JobSpec {
    pub id: i32,
    pub steps: i32,
    pub weight: i32,
}

pub trait EventSink {
    fn on_complete(self: &mut Self, job_id: i32, value: i32, ticks: i32) -> i32;
    fn on_cancel(self: &mut Self, job_id: i32, ticks: i32) -> i32;
    fn summary_metric(self: &Self) -> i32;
}

// Copy sink tracking completed/cancelled counters and accumulated checksum.
pub struct CounterSink {
    pub completed: i32,
    pub cancelled: i32,
    pub checksum: i32,
}

impl EventSink for CounterSink {
    fn on_complete(self: &mut Self, job_id: i32, value: i32, ticks: i32) -> i32 {
        self.completed += 1
        self.checksum += job_id * 10 + value + ticks
        return self.completed
    }

    fn on_cancel(self: &mut Self, job_id: i32, ticks: i32) -> i32 {
        self.cancelled += 1
        self.checksum += job_id + ticks
        return self.cancelled
    }

    fn summary_metric(self: &Self) -> i32 {
        return self.checksum
    }
}

// Owning sink recording structured event lines into a managed ByteBuffer.
pub package AuditLogSink {
    manages log: ByteBuffer;
    pub events: i32
}

fn append_bytes(dst: ByteBuffer, src: &[u8]) -> bool {
    var i: i64 = 0
    while i < src.len() {
        if !bytes_push(dst, src[i]) {
            return false
        }
        i += 1
    }
    return true
}

fn append_i32(dst: ByteBuffer, v: i32) -> bool {
    let dec = int.format_i32(v)
    var i: i64 = 0
    while i < dec.len {
        if !bytes_push(dst, dec.bytes[i]) {
            return false
        }
        i += 1
    }
    return true
}

impl EventSink for AuditLogSink {
    fn on_complete(self: &mut Self, job_id: i32, value: i32, ticks: i32) -> i32 {
        self.events += 1
        let _ = append_bytes(self.log, "OK#")
        let _ = append_i32(self.log, job_id)
        let _ = append_bytes(self.log, "=")
        let _ = append_i32(self.log, value)
        let _ = append_bytes(self.log, "@")
        let _ = append_i32(self.log, ticks)
        let _ = append_bytes(self.log, ";")
        return self.events
    }

    fn on_cancel(self: &mut Self, job_id: i32, ticks: i32) -> i32 {
        self.events += 1
        let _ = append_bytes(self.log, "CANCEL#")
        let _ = append_i32(self.log, job_id)
        let _ = append_bytes(self.log, "@")
        let _ = append_i32(self.log, ticks)
        let _ = append_bytes(self.log, ";")
        return self.events
    }

    fn summary_metric(self: &Self) -> i32 {
        return self.events * 1000 + i32(bytes_len(self.log))
    }
}

pub fn build_default_sinks() -> SchedulerError!Store<OwnedDyn<dyn EventSink>> contract { result(create) } {
    let sinks: Store<OwnedDyn<dyn EventSink>> = store_new()
    if !store_reserve(sinks, 2) {
        return SchedulerError.Allocation
    }
    match owned_dyn::<dyn EventSink>(CounterSink { completed: 0, cancelled: 0, checksum: 0 }) {
        .err(_) => { return SchedulerError.Allocation },
        .ok(counter) => {
            match store_push(sinks, counter) {
                .err(_) => { return SchedulerError.Allocation },
                .ok(_) => {}
            }
        }
    }
    let buf = bytes_new()
    if !bytes_reserve(buf, 128) {
        return SchedulerError.Allocation
    }
    match owned_dyn::<dyn EventSink>(AuditLogSink { log: give buf, events: 0 }) {
        .err(_) => { return SchedulerError.Allocation },
        .ok(audit) => {
            match store_push(sinks, audit) {
                .err(_) => { return SchedulerError.Allocation },
                .ok(_) => {}
            }
        }
    }
    return give sinks
}

// Cooperative suspending helper performing one unit of work and yielding.
// Used by `cooperative_worker` to exercise multi-frame task suspension and
// deterministic `defer` unwind on cancellation.
fn run_work_step(acc: &mut i32, step: i32, weight: i32) {
    *acc += (step + 1) * weight
    task_yield()
}

// Transfer-safe cooperative worker: owns a scratch ByteBuffer to prove
// deterministic single-drop cleanup on both normal completion and mid-flight cancel.
pub fn cooperative_worker(job_id: i32, steps: i32, weight: i32) -> i32 {
    let scratch = bytes_new()
    let _ = bytes_reserve(scratch, 16)
    let _ = bytes_push(scratch, u8(65 + (job_id % 26)))
    var acc = i32(bytes_len(scratch)) - 1
    var step = 0
    while step < steps {
        run_work_step(&mut acc, step, weight)
        step += 1
    }
    return acc + job_id
}

fn trim_slice(s: &[u8]) -> &[u8] {
    var lo: i64 = 0
    var hi: i64 = s.len()
    while lo < hi && (s[lo] == 32u8 || s[lo] == 9u8 || s[lo] == 13u8) {
        lo += 1
    }
    while hi > lo && (s[hi - 1] == 32u8 || s[hi - 1] == 9u8 || s[hi - 1] == 13u8) {
        hi -= 1
    }
    return s[lo..hi]
}

pub fn parse_manifest_line(line: &[u8], default_id: i32) -> SchedulerError!Option<JobSpec> {
    let trimmed = trim_slice(line)
    if trimmed.len() == 0 || trimmed[0] == 35u8 {
        return Option.None()
    }
    // Format: "<steps> <weight>" or "<id> <steps> <weight>"
    var fields = store_new()
    var i: i64 = 0
    while i < trimmed.len() {
        while i < trimmed.len() && (trimmed[i] == 32u8 || trimmed[i] == 9u8) {
            i += 1
        }
        if i >= trimmed.len() {
            break
        }
        let start = i
        while i < trimmed.len() && trimmed[i] != 32u8 && trimmed[i] != 9u8 {
            i += 1
        }
        match int.parse_i32(trimmed[start..i]) {
            .err(_) => { return SchedulerError.InvalidManifest },
            .ok(val) => {
                if !store_reserve(fields, store_len(fields) + 1) {
                    return SchedulerError.Allocation
                }
                match store_push(fields, val) {
                    .err(_) => { return SchedulerError.Allocation },
                    .ok(_) => {}
                }
            }
        }
    }
    let count = store_len(fields)
    if count == 2 {
        let steps = *store_at(fields, 0)
        let weight = *store_at(fields, 1)
        if steps < 0 || weight < 0 {
            return SchedulerError.InvalidManifest
        }
        return Option.Some(JobSpec { id: default_id, steps: steps, weight: weight })
    }
    if count == 3 {
        let id = *store_at(fields, 0)
        let steps = *store_at(fields, 1)
        let weight = *store_at(fields, 2)
        if id <= 0 || steps < 0 || weight < 0 {
            return SchedulerError.InvalidManifest
        }
        return Option.Some(JobSpec { id: id, steps: steps, weight: weight })
    }
    return SchedulerError.InvalidManifest
}
