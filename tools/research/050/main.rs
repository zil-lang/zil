mod fixtures;
mod model;
mod oracle;
mod solve;
#[cfg(test)]
mod tests;

use fixtures::Case;
use model::{Answer, Error, Limits, Result, Stats, check_answer};
use solve::{Cache, Plan};
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Default)]
struct Timing {
    build: u128,
    plan: u128,
    solve: u128,
    check: u128,
    teardown: u128,
    total: u128,
}

fn once(line: &str, cut: bool, expected: Option<&[u64]>) -> Result<(Answer, Stats, Timing)> {
    let start = Instant::now();
    let case = Case::parse(line)?;
    let model = fixtures::build(&case)?;
    let limits = Limits::default();
    model.validate(limits)?;
    let built = Instant::now();
    let mut stats = Stats::default();
    let (answer, planned, solved) = if cut {
        let plan = Plan::build(&model)?;
        let planned = Instant::now();
        let cache = Cache::execute(&model, plan, limits, &mut stats)?;
        let solved = Instant::now();
        (cache.answer()?, planned, solved)
    } else {
        let planned = Instant::now();
        let answer = solve::flat(&model, limits, &mut stats)?;
        let solved = Instant::now();
        (answer, planned, solved)
    };
    check_answer(&model, &answer)?;
    if expected.is_some_and(|rows| rows != answer.relation.rows) {
        return Err(Error::Invalid("frozen full output mismatch".into()));
    }
    let checked = Instant::now();
    Ok((
        answer,
        stats,
        Timing {
            build: built.duration_since(start).as_nanos(),
            plan: planned.duration_since(built).as_nanos(),
            solve: solved.duration_since(planned).as_nanos(),
            check: checked.duration_since(solved).as_nanos(),
            teardown: 0,
            total: checked.duration_since(start).as_nanos(),
        },
    ))
}

fn lines(file: &str) -> Result<Vec<String>> {
    let text = fs::read_to_string(file).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

fn stats_json(s: &Stats) -> String {
    format!(
        "{{\"work\":{},\"max_rows\":{},\"max_scope\":{},\"generated_rows\":{},\"stored_summary_rows\":{},\"nodes_recomputed\":{}}}",
        s.work,
        s.max_rows,
        s.max_scope,
        s.generated_rows,
        s.stored_summary_rows,
        s.nodes_recomputed
    )
}

fn freeze(cases: &str, out: &Path) -> Result<()> {
    fs::create_dir_all(out).map_err(|e| Error::Invalid(e.to_string()))?;
    for line in lines(cases)? {
        let c = Case::parse(&line)?;
        let m = fixtures::build(&c)?;
        m.validate(Limits::default())?;
        let (reference, nodes) = oracle::enumerate(&m, Limits::default())?;
        let (cut, cut_stats, _) = once(&line, true, Some(&reference.rows))?;
        let (flat, flat_stats, _) = once(&line, false, Some(&reference.rows))?;
        if cut.relation != reference || flat.relation != reference {
            return Err(Error::Invalid("freeze oracle relation".into()));
        }
        let rows = reference
            .rows
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>();
        fs::write(out.join(format!("{}.model", c.name)), m.canonical())
            .map_err(|e| Error::Invalid(e.to_string()))?;
        fs::write(out.join(format!("{}.rows", c.name)), rows)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let mut edges = std::collections::BTreeSet::new();
        for f in &m.factors {
            for (i, &a) in f.scope.iter().enumerate() {
                for &b in &f.scope[i + 1..] {
                    edges.insert((a, b));
                }
            }
        }
        println!(
            "{{\"case\":\"{}\",\"recipe\":\"{}\",\"variables\":{},\"factors\":{},\"primal_edges\":{},\"observed\":{},\"input_rows\":{},\"output_rows\":{},\"oracle_nodes\":{},\"planned_max_bag\":{},\"cut\":{},\"flat\":{}}}",
            c.name,
            line,
            m.domains.len(),
            m.factors.len(),
            edges.len(),
            m.observed.len(),
            m.factors.iter().map(|f| f.rows.len()).sum::<usize>(),
            reference.rows.len(),
            nodes,
            Plan::build(&m)?.width,
            stats_json(&cut_stats),
            stats_json(&flat_stats)
        );
    }
    Ok(())
}

fn bench(cases: &str, input: &Path) -> Result<()> {
    for line in lines(cases)? {
        let c = Case::parse(&line)?;
        if c.batch == 0 || c.batch > 64 {
            return Err(Error::Invalid("batch bounds".into()));
        }
        let text = fs::read_to_string(input.join(format!("{}.rows", c.name)))
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let expected = text
            .lines()
            .map(|x| {
                x.parse::<u64>()
                    .map_err(|_| Error::Invalid("frozen rows".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        // Identity is checked before the timing loop; each timed solve compares
        // all output rows and validates a full witness for each output.
        let actual_model = fixtures::build(&c)?.canonical();
        let expected_model = fs::read_to_string(input.join(format!("{}.model", c.name)))
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if actual_model != expected_model {
            return Err(Error::Invalid("frozen input mismatch".into()));
        }
        once(&line, true, Some(&expected))?;
        once(&line, false, Some(&expected))?;
        for round in 0..6 {
            for sample in 0..7 {
                for phase in 0..2 {
                    let cut = (round + sample + phase) % 2 == 0;
                    let mut sum = Timing::default();
                    for _ in 0..c.batch {
                        let operation_start = Instant::now();
                        let (answer, stats, t) = once(&line, cut, Some(&expected))?;
                        drop(answer);
                        drop(stats);
                        let operation_ns = operation_start.elapsed().as_nanos();
                        sum.build += t.build;
                        sum.plan += t.plan;
                        sum.solve += t.solve;
                        sum.check += t.check;
                        sum.teardown += operation_ns - t.total;
                        sum.total += operation_ns;
                    }
                    println!(
                        "{{\"case\":\"{}\",\"method\":\"{}\",\"round\":{},\"sample\":{},\"batch\":{},\"batch_build_ns\":{},\"batch_plan_ns\":{},\"batch_solve_ns\":{},\"batch_check_ns\":{},\"batch_teardown_ns\":{},\"batch_total_ns\":{}}}",
                        c.name,
                        if cut { "cut" } else { "flat" },
                        round,
                        sample,
                        c.batch,
                        sum.build,
                        sum.plan,
                        sum.solve,
                        sum.check,
                        sum.teardown,
                        sum.total
                    );
                }
            }
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let result = if args.len() == 4 {
        match args[1].as_str() {
            "freeze" => freeze(&args[2], Path::new(&args[3])),
            "bench" => bench(&args[2], Path::new(&args[3])),
            _ => Err(Error::Invalid("mode".into())),
        }
    } else {
        Err(Error::Invalid(
            "usage: cut050 {freeze|bench} cases.txt input-directory".into(),
        ))
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
