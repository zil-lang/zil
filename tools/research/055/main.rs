#![forbid(unsafe_code)]
mod block;
mod budget;
mod graph;
mod oracle;
#[cfg(test)]
mod schema8_tests;
#[cfg(test)]
mod tests;

use graph::{Graph, Plan, R, Row};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile, json_string};
use zil_ir::{TypedFunction, TypedModule};

fn typed(name: &str, text: &str) -> R<TypedModule> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let diagnostics = |ds: Vec<zil_base::Diagnostic>| {
        ds.iter()
            .map(|d| d.to_json())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let syntax = zil_syntax::parse(&source)
        .into_result()
        .map_err(diagnostics)?;
    let core = zil_expand::expand(&source, &syntax).map_err(diagnostics)?;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).map_err(diagnostics)
}
fn production(module: TypedModule) -> R<()> {
    let lim = Limits::default();
    let p = zil_analyze::analyze(&module, &lim).map_err(|e| format!("analyze: {e:?}"))?;
    let v = zil_verify::verify_module(module, p, &TargetProfile::host_llvm(), &lim)
        .map_err(|e| format!("verify: {e:?}"))?;
    std::hint::black_box(v);
    Ok(())
}
fn oracle_rows(module: &TypedModule, f: &TypedFunction, g: &Graph) -> R<Vec<Row>> {
    let raw = oracle::solve(module, f)?;
    let mut work = budget::Work::new(budget::LIMIT, "oracle row conversion");
    work.charge(
        g.nodes.len().saturating_mul(
            g.slots
                .len()
                .saturating_add(g.slots.len().div_ceil(64))
                .saturating_add(1),
        ),
    )?;
    let original = f
        .blocks
        .iter()
        .map(|b| b.instructions.len() + 1)
        .sum::<usize>();
    let mut result = vec![vec![0; g.slots.len().div_ceil(64)]; g.nodes.len()];
    for (p, node) in g.nodes[..original].iter().enumerate() {
        for (slot, key) in g.slots.iter().enumerate() {
            if raw[&(node.block, node.ordinal)].contains(key) {
                result[p][slot / 64] |= 1u64 << (slot % 64);
            }
        }
    }
    Ok(result)
}
fn snapshot(g: &Graph, p: &Plan, rows: &[Row]) -> R<String> {
    let mut work = budget::Work::new(budget::LIMIT, "full output");
    work.charge(
        g.nodes
            .len()
            .saturating_add(rows.len())
            .saturating_add(g.slots.len().saturating_mul(64)),
    )?;
    if rows.len() != g.nodes.len() {
        return Err("missing full relation rows".into());
    }
    let words = g.slots.len().div_ceil(64);
    let used = g.slots.len() % 64;
    work.charge(rows.len().saturating_mul(4))?;
    if rows
        .iter()
        .any(|r| r.len() != words || (used != 0 && r.last().is_some_and(|w| w >> used != 0)))
    {
        return Err("malformed full relation row".into());
    }
    let mut out = format!(
        "function {} entry {} slots {:?}\n",
        g.function, g.entry, g.slots
    );
    for (i, (node, row)) in g.nodes.iter().zip(rows).enumerate() {
        work.charge(
            512usize
                .saturating_add(row.len().saturating_mul(128))
                .saturating_add(node.label.len().saturating_mul(8)),
        )?;
        out.push_str(&format!(
            "point {i} {}:{} reachable={} {:?}\n",
            node.block, node.ordinal, g.reachable[i], row
        ));
        if node.observable {
            out.push_str(&format!("event {i} {}\n", node.label));
        }
    }
    for edge in graph::boundaries(g, p)? {
        work.charge(512)?;
        out.push_str(&format!("edge {edge:?}\n"));
    }
    Ok(out)
}

#[derive(Clone)]
struct Case {
    name: String,
    path: String,
    origin: String,
    text: String,
}
fn cases(path: &Path) -> R<Vec<Case>> {
    fs::read_to_string(path)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let p = l.split('\t').collect::<Vec<_>>();
            if p.len() != 3 {
                return Err(format!("bad recipe {l}"));
            }
            Ok(Case {
                name: p[0].into(),
                path: p[1].into(),
                origin: p[2].into(),
                text: fs::read_to_string(p[1]).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}
fn freeze(case: &Case, out: &Path) -> R<()> {
    let module = typed(&case.path, &case.text)?;
    production(module.clone())?;
    let mut expected = String::new();
    let mut details = vec![];
    for f in &module.functions {
        if f.foreign.is_some() {
            continue;
        }
        let g = graph::extract(&module, f)?;
        oracle::audit_extraction(&module, f, &g)?;
        let flat = graph::plan(&g, false)?;
        let cut = graph::plan(&g, true)?;
        let rows = oracle_rows(&module, f, &g)?;
        let observed = snapshot(&g, &flat, &rows)?;
        for method in ["flat", "cut", "block"] {
            let (actual, _) = match method {
                "flat" => graph::solve(&g, &flat)?,
                "cut" => graph::solve(&g, &cut)?,
                _ => block::solve(&g, f)?,
            };
            if actual != rows {
                return Err(format!(
                    "{} {} {method} differs from raw oracle",
                    case.name, f.name
                ));
            }
            if snapshot(&g, if method == "cut" { &cut } else { &flat }, &actual)? != observed {
                return Err(format!("{} {} event relation differs", case.name, f.name));
            }
        }
        let (vertices, edges, component, density, width) = graph::shape(&g)?;
        let mut census_work = budget::Work::new(budget::LIMIT, "census output");
        census_work.charge(
            rows.len()
                .saturating_mul(g.slots.len().div_ceil(64).saturating_add(8))
                .saturating_add(f.name.len().saturating_mul(8))
                .saturating_add(1024),
        )?;
        let live_counts = rows
            .iter()
            .map(|r| r.iter().map(|w| w.count_ones() as usize).sum::<usize>())
            .collect::<Vec<_>>();
        let max_live = live_counts.iter().copied().max().unwrap_or(0);
        let max_cut_live = cut
            .removed
            .iter()
            .map(|(id, _)| live_counts[*id])
            .max()
            .unwrap_or(0);
        let full_facts = live_counts.iter().sum::<usize>();
        let relation_bytes = rows
            .iter()
            .map(|r| r.len() * std::mem::size_of::<u64>())
            .sum::<usize>();
        details.push(format!("{{\"function\":{},\"name\":{},\"blocks\":{},\"slots\":{},\"nodes\":{},\"observed_events\":{},\"implicit_failure_sinks\":{},\"edges\":{},\"kept\":{},\"removed\":{},\"remaining_edges\":{},\"max_separator\":{},\"max_fill\":{},\"fill_fallbacks\":{},\"interaction_vertices\":{},\"interaction_edges\":{},\"largest_component\":{},\"interaction_density\":{},\"min_fill_width_upper_bound\":{}}}",f.id.0,json_string(&f.name),f.blocks.len(),g.slots.len(),g.nodes.len(),g.nodes.iter().filter(|n|n.observable).count(),g.implicit_exits,cut.metrics.edges,cut.metrics.kept,cut.metrics.eliminated,cut.metrics.remaining_edges,cut.metrics.max_separator,cut.metrics.max_fill,cut.metrics.retained_for_fill,vertices,edges,component,density,width.map_or("null".into(),|x|x.to_string())));
        let last = details.last_mut().expect("function census");
        last.pop();
        last.push_str(&format!(",\"max_live_slots\":{max_live},\"max_cut_live_slots\":{max_cut_live},\"complete_live_facts\":{full_facts},\"row_payload_bytes\":{relation_bytes}}}"));
        expected.push_str(&observed);
    }
    fs::write(out.join(format!("{}.expected", case.name)), expected).map_err(|e| e.to_string())?;
    fs::write(
        out.join(format!("{}.typed", case.name)),
        format!("{module:#?}"),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{{\"case\":{},\"source\":{},\"origin\":{},\"source_bytes\":{},\"typed_fingerprint\":{},\"production_verified\":true,\"full_shadow_equal\":true,\"functions\":[{}]}}",
        json_string(&case.name),
        json_string(&case.path),
        json_string(&case.origin),
        case.text.len(),
        json_string(&module.fingerprint().to_hex()),
        details.join(",")
    );
    Ok(())
}

#[derive(Default)]
struct Times {
    front: u128,
    extract: u128,
    plan: u128,
    solve: u128,
    check: u128,
    drop: u128,
    total: u128,
    work: usize,
}
fn once(case: &Case, method: &str, expected: &str) -> R<Times> {
    let total = Instant::now();
    let mut times = Times::default();
    let start = Instant::now();
    let module = typed(&case.path, &case.text)?;
    times.front = start.elapsed().as_nanos();
    if method == "production" {
        let start = Instant::now();
        production(module)?;
        times.solve = start.elapsed().as_nanos();
        times.total = total.elapsed().as_nanos();
        return Ok(times);
    }
    let mut output = String::new();
    for f in &module.functions {
        if f.foreign.is_some() {
            continue;
        }
        let start = Instant::now();
        let g = graph::extract(&module, f)?;
        times.extract += start.elapsed().as_nanos();
        let start = Instant::now();
        oracle::audit_extraction(&module, f, &g)?;
        times.check += start.elapsed().as_nanos();
        let start = Instant::now();
        let p = graph::plan(&g, method == "cut")?;
        times.plan += start.elapsed().as_nanos();
        let start = Instant::now();
        let (rows, work) = if method == "block" {
            block::solve(&g, f)?
        } else {
            graph::solve(&g, &p)?
        };
        times.solve += start.elapsed().as_nanos();
        times.work += work;
        let start = Instant::now();
        output.push_str(&snapshot(&g, &p, &rows)?);
        times.check += start.elapsed().as_nanos();
        let start = Instant::now();
        drop(rows);
        drop(p);
        drop(g);
        times.drop += start.elapsed().as_nanos();
    }
    let start = Instant::now();
    if output != expected {
        return Err(format!("{} {method}: frozen output mismatch", case.name));
    }
    times.check += start.elapsed().as_nanos();
    let start = Instant::now();
    drop(output);
    drop(module);
    times.drop += start.elapsed().as_nanos();
    times.total = total.elapsed().as_nanos();
    Ok(times)
}
fn run() -> R<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: cut055 freeze|bench CASES_TSV INPUT_DIRECTORY".into());
    }
    let all = cases(Path::new(&args[1]))?;
    let dir = PathBuf::from(&args[2]);
    match args[0].as_str() {
        "freeze" => {
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            for case in &all {
                freeze(case, &dir)?;
            }
        }
        "bench" => {
            let expected = all
                .iter()
                .map(|c| {
                    fs::read_to_string(dir.join(format!("{}.expected", c.name)))
                        .map_err(|e| e.to_string())
                })
                .collect::<R<Vec<_>>>()?;
            let methods = ["flat", "cut", "block", "production"];
            for (case, expected) in all.iter().zip(&expected) {
                for method in methods {
                    std::hint::black_box(once(case, method, expected)?);
                }
            }
            for round in 0..6 {
                for sample in 0..7 {
                    for (index, case) in all.iter().enumerate() {
                        for offset in 0..4 {
                            let method = methods[(offset + round + sample) % 4];
                            let t = once(case, method, &expected[index])?;
                            println!(
                                "{{\"case\":{},\"method\":{},\"round\":{},\"sample\":{},\"front_ns\":{},\"extract_ns\":{},\"plan_ns\":{},\"solve_ns\":{},\"check_ns\":{},\"drop_ns\":{},\"total_ns\":{},\"logical_solve_work\":{}}}",
                                json_string(&case.name),
                                json_string(method),
                                round,
                                sample,
                                t.front,
                                t.extract,
                                t.plan,
                                t.solve,
                                t.check,
                                t.drop,
                                t.total,
                                t.work
                            );
                        }
                    }
                }
            }
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
