//! Complete observable output and cost receipts; no semantics shared with the oracle.
use crate::{
    bdd,
    budget::{self, R, Work},
    compare,
    encoding::Representation,
    oracle, schema, source, symbolic,
};
use std::{collections::BTreeMap, fmt::Write, path::Path, time::Instant};

pub fn quote(value: &str) -> String {
    let mut result = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < ' ' => write!(&mut result, "\\u{:04x}", c as u32).expect("String write"),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
pub(crate) fn object(fields: Vec<(&str, String)>) -> String {
    format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(k, v)| format!("{}:{v}", quote(k)))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn outcome(error: &str) -> &'static str {
    if error.starts_with("ResourceLimit:") {
        "ResourceLimit"
    } else if error.starts_with("Unsupported:") {
        "Unsupported"
    } else if error.starts_with("Mismatch:") {
        "Mismatch"
    } else {
        "Error"
    }
}

/// Canonical full rows and raw event labels. The shared function encodes values;
/// it does not extract coordinates, interpret an instruction, or choose a row.
pub(super) fn canonical(
    s: &schema::Schema,
    labels: &[String],
    rows: &schema::Relations,
    status: schema::QueryStatus,
    work: &mut Work,
) -> R<String> {
    work.charge(
        s.fields
            .len()
            .saturating_add(labels.len())
            .saturating_add(rows.len()),
    )?;
    let mut bound = 1024usize;
    for k in s.fields.keys() {
        bound = bound.saturating_add(k.len().saturating_mul(6).saturating_add(64));
    }
    for k in labels {
        bound = bound.saturating_add(k.len().saturating_mul(6).saturating_add(8));
    }
    for (key, set) in rows {
        bound = bound.saturating_add(key.len().saturating_mul(6).saturating_add(16));
        bound = bound.saturating_add(
            set.len()
                .saturating_mul(s.bits.div_ceil(64).saturating_mul(22).saturating_add(8)),
        );
    }
    if bound > 64 * 1024 * 1024 {
        return Err("ResourceLimit: serialized relation bytes".into());
    }
    work.charge(bound.saturating_mul(2))?; // construction and eventual release
    let mut out = String::with_capacity(bound);
    write!(
        &mut out,
        "{{\"status\":{},\"bits\":{},\"fields\":{{",
        quote(&format!("{status:?}")),
        s.bits
    )
    .unwrap();
    for (i, (key, (at, width))) in s.fields.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        write!(&mut out, "{}:[{at},{width}]", quote(key)).unwrap();
    }
    out.push_str("},\"events\":[");
    for (i, label) in labels.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        out.push_str(&quote(label));
    }
    out.push_str("],\"relations\":{");
    for (i, (key, set)) in rows.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        write!(&mut out, "{}:[", quote(key)).unwrap();
        for (j, row) in set.iter().enumerate() {
            if j != 0 {
                out.push(',');
            }
            write!(&mut out, "{row:?}").unwrap();
        }
        out.push(']');
    }
    out.push_str("}}");
    if out.len() > bound {
        return Err("InternalError: serialization bound".into());
    }
    Ok(out)
}

pub(super) fn materialize(s: &mut symbolic::Solution, o: &oracle::Reference) -> R<String> {
    let keys = if let Some(relations) = &s.explicit {
        relations.keys().cloned().collect::<Vec<_>>()
    } else {
        s.relations.keys().cloned().collect::<Vec<_>>()
    };
    s.bdd.work.charge(keys.iter().map(|k| k.len() + 1).sum())?;
    let mut rows = BTreeMap::new();
    let mut total = 0;
    for key in &keys {
        let physical = if let Some(relations) = &s.explicit {
            let set = &relations[key];
            s.bdd.work.charge(
                set.len()
                    .saturating_mul(s.encoding.physical.bits.div_ceil(64) * 2 + 1),
            )?;
            set.clone()
        } else {
            s.bdd.rows(s.relations[key], budget::MAX_ROWS - total)?
        };
        let count = physical.len();
        let mut set = std::collections::BTreeSet::new();
        for row in physical {
            // Charge decoded-row ordering/comparison as well as the codec.
            // These are deterministic abstract work units, not CPU instructions.
            let comparisons = (set.len() + 1).ilog2() as usize + 1;
            s.bdd
                .work
                .charge(comparisons.saturating_mul(s.layout.schema.bits.div_ceil(64) + 1))?;
            set.insert(s.encoding.decode(&row, &mut s.bdd.work)?);
        }
        if set.len() != count {
            return Err("Mismatch: noninjective decoded relation".into());
        }
        total += set.len();
        s.bdd.work.charge(
            key.len().saturating_add(
                set.len()
                    .saturating_mul(s.layout.schema.bits.div_ceil(64).saturating_add(1)),
            ),
        )?;
        rows.insert(key.clone(), set);
    }
    // This compares actual independently materialized candidate rows, in addition
    // to the earlier count + membership proof. No digest is used as an oracle.
    s.bdd
        .work
        .charge(total.saturating_mul(s.layout.schema.bits.div_ceil(64).saturating_add(1)))?;
    if rows != o.relations {
        return Err("Mismatch: materialized complete relation".into());
    }
    canonical(
        &s.layout.schema,
        &s.layout.labels,
        &rows,
        s.status,
        &mut s.bdd.work,
    )
}

pub fn execute(path: &Path, method: &str, output: Option<&Path>) -> R<String> {
    let total_start = Instant::now();
    let front = Instant::now();
    let (m, files) = source::load(path)?;
    let frontend_ns = front.elapsed().as_nanos();
    if m.schema_version != 10 {
        return Err("Unsupported: executable requires schema 10".into());
    }
    let source_files = format!(
        "[{}]",
        files
            .iter()
            .map(|(p, _)| quote(&p.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(",")
    );
    if method == "production" {
        let start = Instant::now();
        let limits = zil_base::Limits::default();
        let plan = zil_analyze::analyze(&m, &limits);
        let analyze_ns = start.elapsed().as_nanos();
        let verify_ran = plan.is_ok();
        let verify_started = Instant::now();
        let result = plan.and_then(|plan| {
            zil_verify::verify_module(m, plan, &zil_base::TargetProfile::host_llvm(), &limits)
                .map(|_| ())
        });
        let verify_ns = verify_started.elapsed().as_nanos();
        let production_ns = start.elapsed().as_nanos();
        let diagnostics = match &result {
            Ok(_) => "[]".to_string(),
            Err(errors) => format!(
                "[{}]",
                errors
                    .iter()
                    .map(|e| e.to_json())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        };
        return Ok(object(vec![
            ("method", quote(method)),
            ("schema", "10".into()),
            ("source_files", source_files),
            (
                "outcome",
                quote(if result.is_ok() {
                    "Accepted"
                } else {
                    "Rejected"
                }),
            ),
            ("frontend_ns", frontend_ns.to_string()),
            ("production_ns", production_ns.to_string()),
            ("analyze_ns", analyze_ns.to_string()),
            ("verify_ns", verify_ns.to_string()),
            ("verify_ran", verify_ran.to_string()),
            ("total_ns", total_start.elapsed().as_nanos().to_string()),
            ("diagnostics", diagnostics),
        ]));
    }
    let parts = method.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || parts[3] != "gc" {
        return Err(
            "Unsupported: expected dense|location|local:fused|direct|intern:ORDER:gc".into(),
        );
    }
    let representation = match parts[0] {
        "dense" => Representation::Dense,
        "location" => Representation::Location,
        "local" => Representation::Local,
        _ => return Err("Unsupported: coordinate representation".into()),
    };
    let algorithm_name = parts[1];
    let variable_order = parts[2];
    let gc = true;
    let algorithm = match algorithm_name {
        "mono" => bdd::Method::Mono,
        "cut" => bdd::Method::Cut,
        "wire" => bdd::Method::Wire,
        "fused" | "direct" | "intern" => bdd::Method::Fused,
        _ => {
            return Err(
                "usage: cut076 SOURCE dense|location|local:mono|cut|wire|fused|direct|intern:ORDER:gc [RELATION_OUTPUT_DIRECTORY]"
                    .into(),
            );
        }
    };
    if let Some(dir) = output {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut functions = Vec::new();
    let mut oracle_total = 0u128;
    let mut candidate_total = 0u128;
    for f in &m.functions {
        if f.foreign.is_some() {
            continue;
        }
        let mut fields = vec![("id", f.id.0.to_string()), ("name", quote(&f.name))];
        let start = Instant::now();
        let reference = oracle::solve(&m, f, budget::LIMIT);
        let oracle_ns = start.elapsed().as_nanos();
        oracle_total += oracle_ns;
        fields.push(("oracle_ns", oracle_ns.to_string()));
        if let Ok(o) = &reference {
            fields.extend([
                ("oracle_work", o.work.to_string()),
                ("rows", o.rows.to_string()),
            ]);
            if let Some(dir) = output {
                let mut w = Work::new(budget::LIMIT);
                let raw = canonical(&o.schema, &o.labels, &o.relations, o.status, &mut w)?;
                std::fs::write(dir.join(format!("f{}.oracle.json", f.id.0)), raw)
                    .map_err(|e| e.to_string())?;
            }
        }
        let candidate_start = Instant::now();
        let mut diagnostic =
            crate::telemetry::Telemetry::new(std::env::var_os("ZIL_071_DIAGNOSTICS").is_some());
        let mut intern_stats = None;
        let solved = if algorithm_name == "intern" {
            crate::intern::solve(
                &m,
                f,
                budget::LIMIT,
                &mut diagnostic,
                variable_order,
                gc,
                representation,
            )
            .map(|(solution, stats)| {
                intern_stats = Some(stats);
                solution
            })
        } else if algorithm_name == "direct" {
            crate::direct::solve(
                &m,
                f,
                budget::LIMIT,
                &mut diagnostic,
                variable_order,
                gc,
                representation,
            )
        } else {
            symbolic::solve_encoded(
                &m,
                f,
                algorithm,
                budget::LIMIT,
                &mut diagnostic,
                variable_order,
                gc,
                representation,
            )
        };
        fields.push(("telemetry", diagnostic.json()));
        let build_solve_ns = candidate_start.elapsed().as_nanos();
        let mut s = match solved {
            Ok(s) => s,
            Err(e) => {
                candidate_total += build_solve_ns;
                fields.extend([
                    ("outcome", quote(outcome(&e))),
                    ("error", quote(&e)),
                    ("candidate_total_ns", build_solve_ns.to_string()),
                    (
                        "oracle_outcome",
                        quote(
                            reference
                                .as_ref()
                                .map_or_else(|e| outcome(e), |_| "Computed"),
                        ),
                    ),
                ]);
                functions.push(object(fields));
                continue;
            }
        };
        if let Some(stats) = &intern_stats {
            fields.push(("intern_stats", stats.json()));
        }
        let phases = s.phase_ns;
        fields.extend([
            ("extract_ns", phases[0].to_string()),
            ("event_ns", phases[1].to_string()),
            ("plan_ns", phases[2].to_string()),
            ("solve_ns", phases[3].to_string()),
            ("build_solve_ns", build_solve_ns.to_string()),
        ]);
        let start = Instant::now();
        let checked = reference
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|o| compare(&mut s, o));
        let check_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        let mut point_support = 0usize;
        let mut point_nodes = 0usize;
        let census = (|| -> R<()> {
            for (key, root) in &s.relations {
                if key.starts_with("point:") {
                    let (support, nodes) = s.bdd.shape(*root)?;
                    point_support = point_support.max(support);
                    point_nodes = point_nodes.max(nodes);
                }
            }
            Ok(())
        })();
        let census_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        let materialized = checked
            .as_ref()
            .map_err(Clone::clone)
            .and(census)
            .and_then(|_| materialize(&mut s, reference.as_ref().expect("checked reference")))
            .and_then(|data| {
                s.bdd.work.charge(data.len())?;
                std::hint::black_box(zil_base::Fingerprint::of(data.as_bytes()));
                Ok(data)
            });
        let materialize_ns = start.elapsed().as_nanos();
        let mut serial = String::new();
        let mut result = match materialized {
            Ok(data) => {
                serial = data;
                format!("{:?}", s.status)
            }
            Err(e) => {
                fields.push(("error", quote(&e)));
                outcome(&e).to_string()
            }
        };
        if let Err(e) = &reference {
            fields.push(("oracle_error", quote(e)));
            if !result.starts_with("Mismatch") {
                result = format!("Oracle{}", outcome(e));
            }
        }
        fields.extend([
            ("outcome", quote(&result)),
            ("check_ns", check_ns.to_string()),
            ("census_ns", census_ns.to_string()),
            ("materialize_ns", materialize_ns.to_string()),
            ("bits", s.layout.schema.bits.to_string()),
            ("physical_bits", s.encoding.physical.bits.to_string()),
            ("encoding", quote(s.encoding.representation.name())),
            (
                "engine",
                quote(if algorithm_name == "intern" {
                    "interned-whole-row"
                } else if s.explicit.is_some() {
                    "direct-whole-row"
                } else {
                    "bdd-relation"
                }),
            ),
            (
                "candidate_work_model",
                quote(if algorithm_name == "intern" {
                    "076-interned-row-v1"
                } else if s.explicit.is_some() {
                    "071-direct-row-v1"
                } else {
                    "063-bdd-plus-071-domain-v1"
                }),
            ),
            ("domain_pairs", s.encoding.domain_pairs.to_string()),
            ("raw_domain_edges", s.encoding.raw_domain_edges.to_string()),
            (
                "max_point_explicit_rows",
                s.explicit
                    .as_ref()
                    .map_or(0, |rs| {
                        rs.iter()
                            .filter(|(k, _)| k.starts_with("point:"))
                            .map(|(_, r)| r.len())
                            .max()
                            .unwrap_or(0)
                    })
                    .to_string(),
            ),
            ("incidence_bits", s.encoding.incidence_bits.to_string()),
            ("location_bits", s.encoding.location_bits.to_string()),
            ("points", s.events.to_string()),
            ("owners", s.layout.owners.len().to_string()),
            ("tokens", s.layout.tokens.len().to_string()),
            ("description_slots", s.layout.descriptions.len().to_string()),
            ("description_roots", s.layout.roots.len().to_string()),
            ("allocated_bdd_nodes", s.bdd.node_count().to_string()),
            ("occupied_bdd_nodes", s.bdd.occupied_nodes().to_string()),
            ("created_bdd_nodes", s.bdd.created_nodes.to_string()),
            ("gc_count", s.bdd.gc_count.to_string()),
            ("gc_collected_nodes", s.bdd.collected_nodes.to_string()),
            ("gc_work", s.bdd.gc_work.to_string()),
            ("max_point_support", point_support.to_string()),
            ("max_point_reachable_bdd_nodes", point_nodes.to_string()),
            ("max_transition_plan_count", s.bdd.max_boundary.to_string()),
            ("max_factor_support", s.bdd.max_factor_support.to_string()),
            ("images", s.bdd.image_calls.to_string()),
            ("wire_substitutions", s.bdd.wire_substitutions.to_string()),
            ("early_eliminations", s.bdd.early_eliminations.to_string()),
            ("candidate_work", s.bdd.work.used.to_string()),
            ("materialized_bytes", serial.len().to_string()),
        ]);
        if let Ok(o) = &reference {
            fields.extend([
                (
                    "fault_relations",
                    o.relations
                        .keys()
                        .filter(|k| k.starts_with("fault:"))
                        .count()
                        .to_string(),
                ),
                (
                    "unsupported_relations",
                    o.relations
                        .keys()
                        .filter(|k| k.starts_with("unsupported:"))
                        .count()
                        .to_string(),
                ),
            ]);
        }
        // File I/O is freeze-only. Sample mode still constructs, fingerprints,
        // observes and destroys every byte of the canonical complete output.
        if let Some(dir) = output
            && !serial.is_empty()
        {
            std::fs::write(dir.join(format!("f{}.json", f.id.0)), &serial)
                .map_err(|e| e.to_string())?;
        }
        let start = Instant::now();
        drop(s);
        drop(serial);
        let drop_ns = start.elapsed().as_nanos();
        let candidate_ns = candidate_start.elapsed().as_nanos();
        candidate_total += candidate_ns;
        fields.extend([
            ("drop_ns", drop_ns.to_string()),
            ("candidate_total_ns", candidate_ns.to_string()),
        ]);
        let start = Instant::now();
        drop(reference);
        let oracle_drop_ns = start.elapsed().as_nanos();
        oracle_total += oracle_drop_ns;
        fields.push(("oracle_drop_ns", oracle_drop_ns.to_string()));
        functions.push(object(fields));
    }
    Ok(object(vec![
        ("method", quote(method)),
        (
            "algorithm",
            quote(
                if algorithm_name == "direct" || algorithm_name == "intern" {
                    algorithm_name
                } else {
                    algorithm.name()
                },
            ),
        ),
        (
            "process_peak_rss_kib",
            process_peak_rss().map_or("null".into(), |n| n.to_string()),
        ),
        (
            "rss_scope",
            quote(
                "Linux process VmHWM; frontend+oracle+candidate; not candidate-only; null if unavailable",
            ),
        ),
        ("representation", quote(representation.name())),
        ("variable_order", quote(variable_order)),
        ("gc", gc.to_string()),
        ("schema", "10".into()),
        ("source_files", source_files),
        ("frontend_ns", frontend_ns.to_string()),
        ("candidate_total_ns", candidate_total.to_string()),
        ("oracle_total_ns", oracle_total.to_string()),
        ("total_ns", total_start.elapsed().as_nanos().to_string()),
        ("functions", format!("[{}]", functions.join(","))),
    ]))
}

fn process_peak_rss() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
    })
}
