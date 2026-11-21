//! Frozen bounded campaigns share an execution protocol and retain independent program models.
use crate::{host, protocol, task_results};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

const WORKER_BUDGET_MS: u64 = 22_000;
const HOST_TIMEOUT_MS: u64 = 24_000;

pub struct Rules {
    pub contract_id: &'static str,
    pub rule_reference: &'static str,
    pub shapes: &'static [&'static str],
    pub exits: &'static [&'static str],
    pub premises: &'static [&'static str],
    pub operations: &'static [&'static str],
    pub max_depth: usize,
    pub rule: &'static str,
    pub cases: fn() -> Vec<(String, Value)>,
    pub contract: fn(&Value) -> Result<Value, String>,
    pub property: fn(&Value) -> &'static str,
}

pub fn task_campaign(root: &Path, commit: &str, jobs: usize) -> Result<Value, String> {
    run(
        &Rules {
            contract_id: task_results::CONTRACT,
            rule_reference: task_results::RULE_REFERENCE,
            shapes: &task_results::SHAPES,
            exits: &task_results::EXITS,
            premises: &task_results::PREMISES,
            operations: &[
                "Task creation with compact or complete worker context",
                "owning relay",
            ],
            max_depth: 1,
            rule: "§12.1.2",
            cases: task_results::fixed,
            contract: task_results::contract,
            property: task_results::property,
        },
        root,
        commit,
        jobs,
    )
}

pub(crate) fn first_compile_failure(evidence: &Value) -> Option<&Value> {
    fn failed_compile(compile: &Value) -> Option<&Value> {
        (!compile["status"].is_null() && compile["status"] != "Success").then_some(compile)
    }
    failed_compile(&evidence["compile"]).or_else(|| {
        ["zil-O0", "zil-O2", "llvm-O0", "llvm-O2"]
            .into_iter()
            .find_map(|route| failed_compile(&evidence["variants"][route]["compile"]))
    })
}

pub(crate) fn compile_location(compile: &Value) -> Value {
    json!({"phase":compile["phase"], "code":compile["diagnostics"][0]["code"],
        "source":compile["diagnostics"][0]["span"], "category":compile["status"]})
}

fn failure_location(result: &Value) -> Option<(&str, &str)> {
    if result["status"] == "pass" {
        return None;
    }
    first_compile_failure(&result["nativeEvidence"][0])
        .map(|compile| {
            (
                compile["phase"].as_str().unwrap_or("compile"),
                compile["diagnostics"][0]["code"]
                    .as_str()
                    .unwrap_or("unspecified"),
            )
        })
        .or_else(|| {
            let issue = &result["issues"][0];
            Some((
                issue["phase"].as_str()?,
                issue["name"].as_str().unwrap_or("unspecified"),
            ))
        })
        .or_else(|| (result["status"] == "fail").then_some(("runtime", "observation-mismatch")))
        .or_else(|| result["status"].as_str().map(|status| ("harness", status)))
}

pub(crate) fn outcome(result: &Value) -> &'static str {
    if result["status"] == "incomplete" || result["reason"] == "time-budget" {
        "budget-exhausted"
    } else if result["status"] == "error" && result["reason"] == "timeout" {
        "host-timeout"
    } else if result["steps"] != 1 {
        "unreached"
    } else if result["status"] == "pass" {
        "matched"
    } else if let Some(compile) = first_compile_failure(&result["nativeEvidence"][0]) {
        match compile["status"].as_str() {
            Some("ResourceLimit") => "budget-exhausted",
            Some("Unsupported") => "unsupported",
            Some("SafeReject") => "rejected-at-first-mismatch",
            _ => "compiler-error",
        }
    } else {
        "observation-mismatch"
    }
}

fn run_case(
    rules: &Rules,
    root: &Path,
    snapshot: &Path,
    toolchain: &Value,
    label: &str,
    case: &Value,
) -> Result<Value, String> {
    let mut case = case.clone();
    crate::native::bind_identity(&mut case, toolchain)?;
    let expected = (rules.contract)(&case["steps"][0]["program"])?["observation"].take();
    let attempt = root.join(label);
    let run = host::supervise(
        &case,
        host::HostOptions {
            command: &snapshot.join("roam"),
            args: vec!["worker".into(), format!("--budget-ms={WORKER_BUDGET_MS}")],
            timeout_ms: HOST_TIMEOUT_MS,
            max_output: protocol::MAX_FRAME,
            attempt: &attempt,
        },
    );
    let (evidence, elapsed_ms) = match run {
        Ok(run) => (run.result, Some(run.elapsed_ms)),
        Err(error) => (json!({"status":"error","reason":error}), None),
    };
    fs::create_dir_all(&attempt).map_err(|e| e.to_string())?;
    let proof = json!({"label":label,"expected":expected,"result":evidence});
    host::durable(&attempt.join("rule-result.json"), &proof)?;
    // Per-case evidence retains the complete diagnostic and codegen receipts.
    let program = &case["steps"][0]["program"];
    let first_failure =
        failure_location(&evidence).map(|(phase, code)| json!({"phase":phase,"code":code}));
    Ok(
        json!({"label":label,"expected":expected,"status":evidence["status"],
        "steps":evidence["steps"],"reason":evidence.get("reason"),
        "shape":program["shape"],"property":(rules.property)(program),"reference":rules.rule_reference,
        "outcome":outcome(&evidence),"exit":program["exit"],"premise":program["premise"],
        "elapsedMs":elapsed_ms,"lastNativeBoundary":evidence.get("lastNativeBoundary"),
        "firstFailure":first_failure,"failure":evidence.get("failure")}),
    )
}

pub fn run(rules: &Rules, root: &Path, commit: &str, jobs: usize) -> Result<Value, String> {
    fs::create_dir(root).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let snapshot = host::freeze(&exe, &root.join("snapshots"), commit)?;
    let worker_sha = snapshot
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("frozen worker identity is missing")?;
    let toolchain = crate::native::identity()?;
    let cases = (rules.cases)();
    let plan = cases
        .iter()
        .map(|(label, case)| {
            let program = &case["steps"][0]["program"];
            Ok(
                json!({"label":label,"program":program,"property":(rules.property)(program),
            "reference":rules.rule_reference,"contract":(rules.contract)(program)?}),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    host::durable(&root.join("expectations.json"), &json!(plan))?;
    // Verify the actual member of each coordinate before pairing a rejection
    // with its legal control. A missing, reordered or duplicated case cannot
    // satisfy an exhaustive bound through matching counts alone.
    let expected_width = rules.exits.len() + rules.premises.len();
    if plan.len() != rules.shapes.len() * expected_width || rules.exits.is_empty() {
        return Err("bounded campaign does not enumerate its declared space".into());
    }
    for (shape, group) in rules.shapes.iter().zip(plan.chunks_exact(expected_width)) {
        for (row, (exit, premise)) in group.iter().zip(
            rules.exits.iter().map(|exit| (*exit, "complete")).chain(
                rules
                    .premises
                    .iter()
                    .map(|premise| (rules.exits[0], *premise)),
            ),
        ) {
            let program = &row["program"];
            if program["shape"] != *shape
                || program["exit"] != exit
                || program["premise"] != premise
            {
                return Err(format!(
                    "invalid bounded campaign coordinate {shape} {exit} {premise}"
                ));
            }
        }
    }
    let mut results = std::thread::scope(|scope| {
        let snapshot = &snapshot;
        let toolchain = &toolchain;
        let cases = &cases;
        let workers: Vec<_> = (0..jobs)
            .map(|worker| {
                scope.spawn(move || {
                    cases
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(jobs)
                        .map(|(index, (label, case))| {
                            run_case(rules, root, snapshot, toolchain, label, case)
                                .map(|row| (index, row))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .collect();
        let mut rows = Vec::with_capacity(cases.len());
        for worker in workers {
            rows.extend(
                worker
                    .join()
                    .map_err(|_| "bounded campaign worker panicked")??,
            );
        }
        rows.sort_by_key(|(index, _)| *index);
        Ok::<Vec<_>, String>(rows.into_iter().map(|(_, row)| row).collect())
    })?;
    let passed = |row: &Value| row["status"] == "pass" && row["steps"] == 1;
    let positive_selected = rules.shapes.len() * rules.exits.len();
    let negative_selected = rules.shapes.len() * rules.premises.len();
    let mut positive_passed = 0;
    let mut negative_passed = 0;
    for family in results.chunks_exact_mut(rules.exits.len() + rules.premises.len()) {
        for row in family.iter_mut() {
            row["publishedScope"] = json!(true);
        }
        positive_passed += family[..rules.exits.len()]
            .iter()
            .filter(|row| passed(row))
            .count();
        let baseline = passed(&family[0]);
        for negative in &mut family[rules.exits.len()..] {
            negative["positiveControlPassed"] = json!(baseline);
            negative_passed += usize::from(baseline && passed(negative));
        }
    }
    let complete = results.len() == positive_selected + negative_selected
        && positive_passed == positive_selected
        && negative_passed == negative_selected;
    let mut failures = BTreeMap::new();
    for row in &results {
        if let Some(phase) = row["firstFailure"]["phase"].as_str() {
            let key = (
                row["property"].as_str().expect("generated property"),
                phase,
                row["firstFailure"]["code"]
                    .as_str()
                    .expect("diagnostic code"),
                row["publishedScope"].as_bool().expect("classified path"),
            );
            let (count, _) = failures.entry(key).or_insert((0, &row["label"]));
            *count += 1;
        }
    }
    let failure_groups: Vec<_> = failures
        .into_iter()
        .map(
            |((property, phase, code, published_scope), (count, case))| {
                json!({"property":property,"phase":phase,"code":code,
                "publishedScope":published_scope,"count":count,"firstCase":case})
            },
        )
        .collect();
    let report = json!({
        "contract":rules.contract_id, "candidate":commit, "roamSha256":worker_sha,
        "execution":{"jobs":jobs,"workerBudgetMs":WORKER_BUDGET_MS,"hostTimeoutMs":HOST_TIMEOUT_MS,
            "clangTimeoutMs":crate::native::CLANG_TIMEOUT_MS,"programTimeoutMs":crate::native::PROGRAM_TIMEOUT_MS,
            "maxOutputBytes":protocol::MAX_FRAME,"workProfile":"alpha","maxAnalysisSteps":crate::native::CALLABLE_ANALYSIS_STEPS},
        "bound":{"maximumDepth":rules.max_depth,"operations":rules.operations,"exits":rules.exits,
            "shapes":rules.shapes, "premises":rules.premises, "rule":rules.rule},
        "status":if complete {"pass"} else {"fail"},
        "positive":{"selected":positive_selected,"passed":positive_passed},
        "negative":{"selected":negative_selected,"passed":negative_passed},
        "targetSpace":{"selection":"exhaustive","sampling":0,
            "positive":{"selected":positive_selected,"passed":positive_passed},
            "negative":{"selected":negative_selected,"passed":negative_passed},"complete":complete},
        "boundary":{"selected":0},
        "failureGroups":failure_groups,
        "cases":results
    });
    host::durable(&root.join("report.json"), &report)?;
    Ok(report)
}
