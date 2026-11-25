// Async Task Scheduler CLI entry point.
// Modes:
//   (no args) | demo                : runs built-in multi-job schedule with budget cancellation
//   batch <manifest> [job_budget]   : loads job specs from file and executes with optional tick budget
//   cancel-stress [rounds]          : stress-tests mid-flight task cancellation and resource teardown
import jobs
import scheduler
import std.env
import std.fs
import std.int
import std.io

fn write_i32(v: i32) {
    let dec = int.format_i32(v)
    let _ = io.print(dec.bytes[0..dec.len])
}

fn print_report(rep: scheduler.ScheduleReport) {
    let _ = io.print("SUMMARY completed=")
    write_i32(rep.completed)
    let _ = io.print(" cancelled=")
    write_i32(rep.cancelled)
    let _ = io.print(" ticks=")
    write_i32(rep.total_ticks)
    let _ = io.print(" checksum=")
    write_i32(rep.counter_checksum)
    let _ = io.print(" audit=")
    write_i32(rep.audit_metric)
    let _ = io.print("\n")
}

fn push_spec(specs: Store<jobs.JobSpec>, id: i32, steps: i32, weight: i32) -> bool {
    if !store_reserve(specs, store_len(specs) + 1) {
        return false
    }
    match store_push(specs, jobs.JobSpec { id: id, steps: steps, weight: weight }) {
        .err(_) => { return false },
        .ok(_) => { return true }
    }
}

fn run_demo() -> i32 {
    let specs: Store<jobs.JobSpec> = store_new()
    // 5 jobs across 3 concurrent slots with per-job budget = 4 ticks:
    // - job 1 (2 steps, weight 3): completes in 3 ticks, value = (1+2)*3 + 1 = 10
    // - job 2 (6 steps, weight 2): exceeds per-job budget (4 ticks) -> cancelled mid-flight!
    // - job 3 (1 step,  weight 5): completes in 2 ticks, value = 1*5 + 3 = 8
    // - job 4 (3 steps, weight 4): completes in 4 ticks, value = (1+2+3)*4 + 4 = 28
    // - job 5 (0 steps, weight 7): completes in 1 tick,  value = 0 + 5 = 5
    if !push_spec(specs, 1, 2, 3) { return 10 }
    if !push_spec(specs, 2, 6, 2) { return 10 }
    if !push_spec(specs, 3, 1, 5) { return 10 }
    if !push_spec(specs, 4, 3, 4) { return 10 }
    if !push_spec(specs, 5, 0, 7) { return 10 }

    match scheduler.run_schedule(specs, 4, 32, true) {
        .err(_) => {
            let _ = io.print("ERROR scheduler failure\n")
            return 1
        },
        .ok(rep) => {
            print_report(rep)
            return 0
        }
    }
}

fn load_manifest_specs(path: &[u8]) -> jobs.SchedulerError!Store<jobs.JobSpec> contract { result(create) } {
    match fs.read_to_bytes(path) {
        .err(_) => { return jobs.SchedulerError.Io },
        .ok(content) => {
            let data = bytes_view(content)
            let specs: Store<jobs.JobSpec> = store_new()
            var next_id = 1
            var start: i64 = 0
            var i: i64 = 0
            while i <= data.len() {
                let at_end = i == data.len()
                if at_end || data[i] == 10u8 {
                    let line = data[start..i]
                    match jobs.parse_manifest_line(line, next_id)? {
                        Option.None() => {},
                        Option.Some(spec) => {
                            if !push_spec(specs, spec.id, spec.steps, spec.weight) {
                                return jobs.SchedulerError.Allocation
                            }
                            next_id += 1
                        }
                    }
                    start = i + 1
                }
                i += 1
            }
            return give specs
        }
    }
}

fn run_batch(path: &[u8], per_job_budget: i32) -> i32 {
    match load_manifest_specs(path) {
        .err(e) => {
            match e {
                jobs.SchedulerError.Io() => {
                    let _ = io.print("ERROR cannot read manifest\n")
                    return 2
                },
                jobs.SchedulerError.InvalidManifest() => {
                    let _ = io.print("ERROR invalid manifest\n")
                    return 3
                },
                jobs.SchedulerError.Allocation() => {
                    let _ = io.print("ERROR allocation\n")
                    return 10
                },
                jobs.SchedulerError.TaskSpawn() => {
                    let _ = io.print("ERROR spawn\n")
                    return 11
                }
            }
        },
        .ok(specs) => {
            match scheduler.run_schedule(specs, per_job_budget, 256, true) {
                .err(_) => {
                    let _ = io.print("ERROR scheduler failure\n")
                    return 1
                },
                .ok(rep) => {
                    print_report(rep)
                    return 0
                }
            }
        }
    }
}

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false
    }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] {
            return false
        }
        i += 1
    }
    return true
}

fn main() -> i32 {
    let argc = env.arg_count()
    if argc <= 1 {
        return run_demo()
    }
    match env.arg(1) {
        Option.None() => { return run_demo() },
        Option.Some(cmd_buf) => {
            let cmd = bytes_view(cmd_buf)
            if bytes_eq(cmd, "demo") {
                return run_demo()
            }
            if bytes_eq(cmd, "cancel-stress") {
                var rounds = 32
                if argc >= 3 {
                    match env.arg(2) {
                        Option.None() => {},
                        Option.Some(r_str) => {
                            match int.parse_i32(bytes_view(r_str)) {
                                .err(_) => {
                                    let _ = io.print("ERROR invalid rounds\n")
                                    return 4
                                },
                                .ok(n) => {
                                    if n <= 0 {
                                        let _ = io.print("ERROR invalid rounds\n")
                                        return 4
                                    }
                                    rounds = n
                                }
                            }
                        }
                    }
                }
                match scheduler.run_cancel_stress(rounds) {
                    .err(_) => {
                        let _ = io.print("ERROR stress failure\n")
                        return 1
                    },
                    .ok(done) => {
                        let _ = io.print("STRESS cancelled=")
                        write_i32(done)
                        let _ = io.print("\n")
                        return 0
                    }
                }
            }
            if bytes_eq(cmd, "batch") {
                if argc < 3 {
                    let _ = io.print("ERROR missing manifest path\n")
                    return 2
                }
                match env.arg(2) {
                    Option.None() => {
                        let _ = io.print("ERROR missing manifest path\n")
                        return 2
                    },
                    Option.Some(path_str) => {
                        var budget = 0
                        if argc >= 4 {
                            match env.arg(3) {
                                Option.None() => {},
                                Option.Some(b_str) => {
                                    match int.parse_i32(bytes_view(b_str)) {
                                        .err(_) => {
                                            let _ = io.print("ERROR invalid budget\n")
                                            return 4
                                        },
                                        .ok(b) => {
                                            if b < 0 {
                                                let _ = io.print("ERROR invalid budget\n")
                                                return 4
                                            }
                                            budget = b
                                        }
                                    }
                                }
                            }
                        }
                        return run_batch(bytes_view(path_str), budget)
                    }
                }
            }
            let _ = io.print("ERROR unknown command\n")
            return 2
        }
    }
}
