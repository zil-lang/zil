//! Exact product relations for the straight-line subset, using actual 076 transfers.
//! Independence is established from expression support AND checked on the initial
//! relation by canonical BDD equality. Branches and alternative transfers reject.
use crate::{
    bdd::{Bdd, Id},
    budget::{self, R},
    direct,
    encoding::{Encoding, Representation},
    extract, oracle, report,
    schema::{QueryStatus, Relations, Rows},
    source,
    symbolic::{self, Event, Solution, Transfer},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
    time::Instant,
};
use zil_ir::{InstructionKind, PassingMode, TypedFunction, TypedModule};

type Product = Vec<Arc<Rows>>;

fn remember(
    relations: &mut BTreeMap<String, Vec<Product>>,
    name: String,
    product: Product,
    index_bytes: &mut usize,
    work: &mut budget::Work,
) -> R<()> {
    *index_bytes = index_bytes.saturating_add(product.len().saturating_mul(24) + name.len() + 128);
    if *index_bytes > 64 * 1024 * 1024 {
        return Err("ResourceLimit: retained product alternatives".into());
    }
    work.charge(product.len().saturating_mul(2) + name.len() + 1)?;
    relations.entry(name).or_default().push(product);
    Ok(())
}

fn root(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}
fn join(parent: &mut [usize], bits: &[usize]) {
    if let Some(&first) = bits.first() {
        let a = root(parent, first);
        for &bit in &bits[1..] {
            let b = root(parent, bit);
            parent[b] = a;
        }
    }
}
fn support(bdd: &mut Bdd, value: Id, order: &[usize]) -> R<Vec<usize>> {
    bdd.support(value)?
        .into_iter()
        .map(|v| {
            if v % 2 != 0 {
                Err("Unsupported: poststate support".into())
            } else {
                Ok(order[usize::from(v / 2)])
            }
        })
        .collect()
}
fn filter(
    product: &Product,
    pred: Id,
    retain: bool,
    bdd: &mut Bdd,
    groups: &[usize],
    order: &[usize],
) -> R<Product> {
    let bits = support(bdd, pred, order)?;
    let group = bits.first().map_or(0, |&b| groups[b]);
    if bits.iter().any(|&b| groups[b] != group) {
        return Err("Mismatch: predicate spans factors".into());
    }
    bdd.work.charge(
        product.len().saturating_mul(2)
            + product[group]
                .iter()
                .map(|row| {
                    (row.len() + 1).saturating_mul(4 + product[group].len().max(1).ilog2() as usize)
                })
                .sum::<usize>(),
    )?;
    let mut out = product.clone();
    let rows = product[group]
        .iter()
        .filter_map(|row| match bdd.contains(pred, row) {
            Ok(value) if value == retain => Some(Ok(row.clone())),
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<R<Rows>>()?;
    out[group] = Arc::new(rows);
    Ok(out)
}
fn count(product: &Product) -> Option<u128> {
    if product.iter().any(|rows| rows.is_empty()) {
        return Some(0);
    }
    product
        .iter()
        .try_fold(1u128, |n, rows| n.checked_mul(rows.len() as u128))
}
fn expand(product: &Product, words: usize, bound: usize) -> R<Rows> {
    if count(product).is_none_or(|n| n > bound as u128) {
        return Err("ResourceLimit: explicit product expansion".into());
    }
    let mut result = Rows::from([vec![0u64; words]]);
    for rows in product {
        let mut next = Rows::new();
        for a in &result {
            for b in rows.iter() {
                next.insert(a.iter().zip(b).map(|(a, b)| a | b).collect());
            }
        }
        result = next;
    }
    Ok(result)
}

fn solve(m: &TypedModule, f: &TypedFunction) -> R<String> {
    let began = Instant::now();
    let admitted = |t: &zil_ir::Type| match t {
        zil_ir::Type::Unit
        | zil_ir::Type::Bool
        | zil_ir::Type::I32
        | zil_ir::Type::I64
        | zil_ir::Type::Managed(_)
        | zil_ir::Type::Desc(_) => true,
        zil_ir::Type::Closure(id) => m.closures.get(id.0 as usize).is_some_and(|c| {
            c.captures.iter().all(|t| {
                matches!(
                    t,
                    zil_ir::Type::Unit
                        | zil_ir::Type::Bool
                        | zil_ir::Type::I32
                        | zil_ir::Type::I64
                        | zil_ir::Type::Managed(_)
                        | zil_ir::Type::Desc(_)
                )
            })
        }),
        _ => false,
    };
    if f.values
        .iter()
        .chain(f.locals.iter().map(|l| &l.ty))
        .chain(std::iter::once(&f.return_type))
        .any(|t| !admitted(t))
    {
        return Err("Unsupported: post-schema10 type outside historical relation profile".into());
    }
    if f.blocks.len() != 1 || f.entry.0 != 0 {
        return Err("Unsupported: factor engine requires one straight-line block".into());
    }
    let mut work = budget::Work::new(budget::LIMIT);
    let layout = extract::extract(m, f, &mut work)?;
    let encoding = Encoding::from_function(&layout, m, f, Representation::Local, &mut work)?;
    let bits = encoding.physical.bits;
    if bits == 0 {
        return Err("Unsupported: empty physical schema".into());
    }
    // Preserve the old owner heuristic; translate support ranks explicitly back to schema coordinates.
    let mut bdd = Bdd::new(bits, budget::LIMIT);
    bdd.work = work;
    let order = crate::ordering::make(&encoding.physical, "owner", &mut bdd.work)?;
    let mut rank = vec![0; bits];
    for (r, &b) in order.iter().enumerate() {
        rank[b] = r;
    }
    bdd.set_order(order.clone())?;
    let mut events = Vec::new();
    for p in 0..layout.points.len() {
        let (_, ordinal) = layout.points[p];
        if ordinal == f.blocks[0].instructions.len() {
            let (transfer, terminal, next) =
                symbolic::terminal(m, f, &layout, &encoding, &mut bdd, p, true)?;
            if !next.is_empty() {
                return Err("Unsupported: nonterminal control edge".into());
            }
            events.push(Event {
                transfers: vec![transfer],
                terminal,
                next,
            });
        } else {
            let ins = &f.blocks[0].instructions[ordinal];
            if let InstructionKind::Call { function, .. } = ins.kind
                && m.functions[function.0 as usize].result_mode == PassingMode::Give
            {
                return Err(
                    "Unsupported: alternative give outcomes require correlated union".into(),
                );
            }
            events.push(Event {
                transfers: vec![symbolic::compile_ins(
                    m, f, &layout, &encoding, &mut bdd, p, false, true,
                )?],
                terminal: vec![],
                next: vec![(p + 1, 1)],
            });
        }
        symbolic::collect_boundary(&mut bdd, &events, &[], &BTreeMap::new(), true)?;
    }
    // Observable labels may have several product alternatives; retain their union.
    let mut parent = (0..bits).collect::<Vec<_>>();
    // Keep each encoded field intact, including finite numeric domains and
    // location codes. This also makes the final canonical codec separable.
    for &(at, width) in encoding.physical.fields.values() {
        join(&mut parent, &(at..at + width).collect::<Vec<_>>());
    }
    let mut event_support = Vec::new();
    for event in &events {
        let mut event_fields = BTreeSet::new();
        for transfer in &event.transfers {
            for &(bit, rhs, free) in &transfer.assignments {
                let mut reads = vec![bit];
                reads.extend(support(&mut bdd, rhs, &order)?);
                reads.extend(support(&mut bdd, free, &order)?);
                event_fields.extend(reads.iter().copied());
                join(&mut parent, &reads);
            }
            for &pred in transfer
                .faults
                .iter()
                .chain(&transfer.aborts)
                .map(|(_, p)| p)
                .chain(std::iter::once(&transfer.invariant_violation))
            {
                let reads = support(&mut bdd, pred, &order)?;
                event_fields.extend(reads.iter().copied());
                join(&mut parent, &reads);
            }
        }
        for &pred in event.terminal.iter().map(|(_, p)| p) {
            let reads = support(&mut bdd, pred, &order)?;
            event_fields.extend(reads.iter().copied());
            join(&mut parent, &reads);
        }
        event_support.push(event_fields);
    }
    let mut components = BTreeMap::<usize, Vec<usize>>::new();
    for bit in 0..bits {
        let r = root(&mut parent, bit);
        components.entry(r).or_default().push(bit);
    }
    let components = components.into_values().collect::<Vec<_>>();
    bdd.work.charge(bits.saturating_mul(components.len() + 3))?;
    let mut groups = vec![0; bits];
    for (group, members) in components.iter().enumerate() {
        for &b in members {
            groups[b] = group;
        }
    }
    if components
        .len()
        .saturating_mul(events.len())
        .saturating_mul(24)
        > 64 * 1024 * 1024
    {
        return Err("ResourceLimit: factored point index".into());
    }
    let initial = symbolic::initial(m, f, &layout, &encoding, &mut bdd)?;
    let mut reconstructed = 1;
    let mut state = Vec::new();
    for (group, members) in components.iter().enumerate() {
        let removed = (0..bits)
            .filter(|&b| groups[b] != group)
            .map(|b| (rank[b] * 2) as u16)
            .collect();
        let projected = bdd.exists(initial, &removed)?;
        reconstructed = bdd.and(reconstructed, projected)?;
        // Enumerate only this factor, forcing all other physical bits to zero.
        let zeros = (0..bits)
            .map(|b| {
                (
                    b,
                    if groups[b] == group {
                        None
                    } else {
                        Some(false)
                    },
                )
            })
            .collect();
        let zero = bdd.cube(&zeros)?;
        let bounded = bdd.and(projected, zero)?;
        state.push(Arc::new(bdd.rows(bounded, budget::MAX_ROWS)?));
        bdd.work.charge(members.len() + 1)?;
        symbolic::collect_boundary(
            &mut bdd,
            &events,
            &[initial, reconstructed],
            &BTreeMap::new(),
            true,
        )?;
    }
    if reconstructed != initial {
        return Err("Unsupported: initial relation couples event-support components".into());
    }
    let mut relations = BTreeMap::<String, Vec<Product>>::new();
    let mut index_bytes = 0;
    for (p, event) in events.iter().enumerate() {
        bdd.work.charge(state.len().saturating_mul(4) + 1)?;
        remember(
            &mut relations,
            format!("point:{p}"),
            state.clone(),
            &mut index_bytes,
            &mut bdd.work,
        )?;
        let t = &event.transfers[0];
        for (label, pred) in &t.aborts {
            let selected = filter(&state, *pred, true, &mut bdd, &groups, &order)?;
            if count(&selected) != Some(0) {
                remember(
                    &mut relations,
                    format!("abort:{p}:{label}"),
                    selected,
                    &mut index_bytes,
                    &mut bdd.work,
                )?;
            }
        }
        for (label, pred) in &t.faults {
            let selected = filter(&state, *pred, true, &mut bdd, &groups, &order)?;
            if count(&selected) != Some(0) {
                if label == "unsupported-overlapping-incarnation" {
                    return Err("Unsupported: multiplicity".into());
                }
                if label != "projection-excludes-invalid-tag" {
                    remember(
                        &mut relations,
                        format!("fault:{p}:{label}"),
                        selected,
                        &mut index_bytes,
                        &mut bdd.work,
                    )?;
                }
            }
        }
        for (_, pred) in &t.faults {
            state = filter(&state, *pred, false, &mut bdd, &groups, &order)?;
        }
        if count(&filter(
            &state,
            t.invariant_violation,
            true,
            &mut bdd,
            &groups,
            &order,
        )?) != Some(0)
        {
            return Err("Mismatch: normal image invariant".into());
        }
        for (group, current) in state.iter_mut().enumerate() {
            let assignments = t
                .assignments
                .iter()
                .copied()
                .filter(|(b, _, _)| groups[*b] == group)
                .collect::<Vec<_>>();
            if assignments.is_empty() {
                continue;
            }
            let transfer = Transfer {
                invariant_violation: 0,
                faults: vec![],
                aborts: vec![],
                factors: vec![],
                assignments,
            };
            let mut rows = Rows::new();
            for pre in current.iter() {
                rows.extend(direct::image(&transfer, pre, &mut bdd)?);
            }
            if rows.len() > budget::MAX_ROWS {
                return Err("ResourceLimit: factor rows".into());
            }
            *current = Arc::new(rows);
        }
        for (label, pred) in &event.terminal {
            let selected = filter(&state, *pred, true, &mut bdd, &groups, &order)?;
            if count(&selected) != Some(0) {
                remember(
                    &mut relations,
                    format!("exit:{p}:{label}"),
                    selected,
                    &mut index_bytes,
                    &mut bdd.work,
                )?;
            }
        }
    }
    let solve_ns = began.elapsed().as_nanos();
    let cardinalities = relations
        .iter()
        .map(|(k, terms)| {
            (
                k.clone(),
                terms
                    .iter()
                    .try_fold(0u128, |n, p| n.checked_add(count(p)?)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let total = cardinalities
        .values()
        .try_fold(0u128, |n, c| n.checked_add((*c)?));
    let explicit_allowed = total.is_some_and(|n| n <= budget::MAX_ROWS as u128);
    let canonical_fields = format!(
        "{{{}}}",
        layout
            .schema
            .fields
            .iter()
            .map(|(k, (at, width))| format!("{}:[{at},{width}]", report::quote(k)))
            .collect::<Vec<_>>()
            .join(",")
    );
    let physical_fields = format!(
        "{{{}}}",
        encoding
            .physical
            .fields
            .iter()
            .map(|(k, (at, width))| format!("{}:[{at},{width}]", report::quote(k)))
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut location_codes = Vec::new();
    for (name, &(at, width)) in &encoding.physical.fields {
        if !name.starts_with("p:") {
            continue;
        }
        let mut codes = vec![];
        for code in 1..(1usize << width) {
            let mut row = vec![0u64; bits.div_ceil(64)];
            for bit in 0..width {
                if code >> bit & 1 != 0 {
                    row[(at + bit) / 64] |= 1 << ((at + bit) % 64);
                }
            }
            let decoded = match encoding.decode(&row, &mut bdd.work) {
                Ok(row) => row,
                Err(error) if error == "Mismatch: unused location code reachable" => break,
                Err(error) => return Err(error),
            };
            let fields = layout
                .schema
                .fields
                .iter()
                .filter_map(|(field, &(bit, w))| {
                    (w == 1 && decoded[bit / 64] >> (bit % 64) & 1 != 0).then_some(field)
                })
                .collect::<Vec<_>>();
            if fields.len() != 1 {
                return Err("Mismatch: location codec is not one incidence field".into());
            }
            codes.push(report::quote(fields[0]));
        }
        location_codes.push(format!("{}:[{}]", report::quote(name), codes.join(",")));
    }
    let component_fields = components
        .iter()
        .map(|members| {
            let mut names = BTreeSet::new();
            for (name, &(at, width)) in &encoding.physical.fields {
                if !members.contains(&at) {
                    continue;
                }
                debug_assert!((at..at + width).all(|b| members.contains(&b)));
                if let Some(token) = name.strip_prefix("p:") {
                    names.extend(
                        layout
                            .schema
                            .fields
                            .keys()
                            .filter(|k| k.starts_with("i:") && k.rsplit(':').next() == Some(token))
                            .cloned(),
                    );
                } else {
                    names.insert(name.clone());
                }
            }
            format!(
                "[{}]",
                names
                    .iter()
                    .map(|n| report::quote(n))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    // Stable factor IDs preserve sharing; every complete row remains reconstructible.
    let mut identities = BTreeMap::<usize, usize>::new();
    let mut factors = Vec::new();
    let mut products = Vec::new();
    let mut serialized_bound = relations
        .values()
        .map(Vec::len)
        .sum::<usize>()
        .saturating_mul(components.len() * 24 + 128);
    for (name, terms) in &relations {
        let mut alternatives = Vec::new();
        for product in terms {
            let mut ids = Vec::new();
            for (group, rows) in product.iter().enumerate() {
                let key = Arc::as_ptr(rows) as usize;
                let id = if let Some(&id) = identities.get(&key) {
                    id
                } else {
                    let id = factors.len();
                    identities.insert(key, id);
                    serialized_bound = serialized_bound
                        .saturating_add(rows.len().saturating_mul(bits.div_ceil(64) * 24 + 8));
                    if serialized_bound > 64 * 1024 * 1024 {
                        return Err("ResourceLimit: compact relation bytes".into());
                    }
                    factors.push(format!(
                        "{{\"group\":{group},\"rows\":{:?}}}",
                        rows.iter().collect::<Vec<_>>()
                    ));
                    id
                };
                ids.push(id);
            }
            alternatives.push(format!("{ids:?}"));
        }
        products.push(format!(
            "{}:[{}]",
            report::quote(name),
            alternatives.join(",")
        ));
    }
    serialized_bound = serialized_bound
        .saturating_add(
            event_support
                .iter()
                .map(|s| s.len() * 24 + 8)
                .sum::<usize>(),
        )
        .saturating_add(
            canonical_fields.len()
                + physical_fields.len()
                + component_fields.len()
                + location_codes.iter().map(String::len).sum::<usize>(),
        );
    if serialized_bound > 64 * 1024 * 1024 {
        return Err("ResourceLimit: complete compact relation bytes".into());
    }
    bdd.work.charge(serialized_bound.saturating_mul(2))?;
    let mut check = "ExpansionDeferred".to_string();
    let mut canonical = String::new();
    if explicit_allowed {
        let explicit = relations
            .iter()
            .map(|(k, terms)| {
                let mut rows = Rows::new();
                for product in terms {
                    rows.extend(expand(product, bits.div_ceil(64), budget::MAX_ROWS)?);
                }
                Ok((k.clone(), rows))
            })
            .collect::<R<Relations>>()?;
        let reference = oracle::solve(m, f, budget::LIMIT)?;
        let mut solution = Solution {
            encoding,
            layout,
            bdd,
            relations: BTreeMap::new(),
            explicit: Some(explicit),
            events: events.len(),
            status: QueryStatus::Complete,
            phase_ns: [0; 4],
        };
        crate::compare(&mut solution, &reference)?;
        canonical = report::materialize(&mut solution, &reference)?;
        check = "ExactOracleRows".into();
    }
    Ok(format!(
        "{{\"function\":{},\"name\":{},\"outcome\":\"CompleteFactored\",\"budget_model\":\"event-support-factor-v1\",\"check\":{},\"canonical_fields\":{canonical_fields},\"physical_fields\":{physical_fields},\"location_codes\":{{{}}},\"component_fields\":[{component_fields}],\"physical_bits\":{bits},\"component_bits\":{:?},\"relations\":{{{}}},\"factors\":[{}],\"event_support\":{:?},\"explicit_total_rows_upper_bound\":{},\"canonical_fingerprint\":{},\"solve_ns\":{solve_ns},\"total_ns\":{}}}",
        f.id.0,
        report::quote(&f.name),
        report::quote(&check),
        location_codes.join(","),
        components,
        products.join(","),
        factors.join(","),
        event_support
            .iter()
            .map(|s| s.iter().copied().collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        total.map_or("null".into(), |n| n.to_string()),
        if explicit_allowed {
            report::quote(&zil_base::Fingerprint::of(canonical.as_bytes()).to_hex())
        } else {
            "null".into()
        },
        began.elapsed().as_nanos()
    ))
}

pub fn run() -> R<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let path = args.first().ok_or("usage: cut-support SOURCE")?;
    let (m, _) = source::load(Path::new(path))?;
    let mut out = Vec::new();
    for f in &m.functions {
        if f.foreign.is_some() {
            continue;
        }
        out.push(solve(&m, f).unwrap_or_else(|error| {
            format!(
                "{{\"function\":{},\"name\":{},\"outcome\":{},\"error\":{}}}",
                f.id.0,
                report::quote(&f.name),
                report::quote(if error.starts_with("ResourceLimit:") {
                    "ResourceLimit"
                } else if error.starts_with("Unsupported:") {
                    "Unsupported"
                } else {
                    "Error"
                }),
                report::quote(&error)
            )
        }));
    }
    println!(
        "{{\"schema\":{},\"query\":\"exact-straight-line-sum-of-products\",\"functions\":[{}]}}",
        m.schema_version,
        out.join(",")
    );
    Ok(())
}
