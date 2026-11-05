//! Actual original-AST deletion events, separate from the public identity table.
use super::*;
use zil_expand::source_context::{DeclarationOrigin, RetainedMemberOrigins};

pub(super) struct MemberRetention {
    host: CtSymbolId,
    map: RetainedMemberOrigins,
}

fn original_host<'a>(
    parsed: &'a [Arc<ParseResult>],
    host: &CtSymbolId,
    budget: &mut zil_ct::CtExecutionBudget,
) -> CompileResult<Option<&'a s::Aggregate>> {
    if host.generation_path.is_some() {
        return Ok(None);
    }
    budget.reserve_analysis_work(parsed.len() as u64 + 1)?;
    let module = parsed
        .iter()
        .find(|module| module.module.id == host.module)
        .ok_or_else(|| category("retained member original module absent", Span::default()))?;
    let original = compiler::source_scope::original_declaration(
        &module.module,
        host.declaration,
        budget.remaining().max_call_depth as usize,
        |work| budget.reserve_analysis_work(work as u64),
    )?
    .ok_or_else(|| category("retained member original host absent", Span::default()))?;
    if !std::ptr::eq(original.item, &module.module.items[original.top]) {
        return Ok(None);
    }
    let s::ItemKind::Aggregate(aggregate) = &original.item.kind else {
        return Ok(None);
    };
    Ok(
        (aggregate.kind == s::AggregateKind::Struct && aggregate.generics.is_empty())
            .then_some(aggregate),
    )
}

// The candidate cannot authenticate itself by being in range. Reconstruct the
// skip decisions from the same immutable original module/host and fresh ct_only.
fn validate_events(
    parsed: &[Arc<ParseResult>],
    ct_only: &[CtSymbolId],
    host: &CtSymbolId,
    input_count: usize,
    kept: &[usize],
    candidate: &RetainedMemberOrigins,
    budget: &mut zil_ct::CtExecutionBudget,
) -> CompileResult<()> {
    let original = original_host(parsed, host, budget)?.ok_or_else(|| {
        category(
            "retained members require an original nongeneric struct",
            Span::default(),
        )
    })?;
    budget.reserve_analysis_work(
        original.methods.len() as u64 + kept.len() as u64 + candidate.positions.len() as u64 + 1,
    )?;
    if original.methods.len() != input_count
        || candidate.original_count as usize != original.methods.len()
        || candidate.positions.len() != kept.len()
    {
        return Err(category(
            "retained member original/event counts differ",
            Span::default(),
        ));
    }
    let module = parsed
        .iter()
        .find(|module| module.module.id == host.module)
        .unwrap();
    let mut output = 0;
    for (ordinal, item) in original.methods.iter().enumerate() {
        let declaration = compiler::source_scope::original_member(
            &module.module,
            host.declaration,
            ordinal,
            budget.remaining().max_call_depth as usize,
            |work| budget.reserve_analysis_work(work as u64),
        )?
        .ok_or_else(|| category("retained member original ordinal absent", item.span))?;
        budget.reserve_analysis_work(ct_only.len() as u64 + 1)?;
        let removed = matches!(
            item.kind,
            s::ItemKind::Function(_) | s::ItemKind::Extern { .. }
        ) && ct_only.iter().any(|symbol| {
            symbol.module == host.module
                && symbol.declaration == declaration
                && symbol.generation_path.is_none()
        });
        if !removed {
            if kept.get(output) != Some(&ordinal)
                || candidate.positions.get(output).map(|n| *n as usize) != Some(ordinal)
            {
                return Err(category(
                    "retained member table differs from actual original skip events",
                    item.span,
                ));
            }
            output += 1;
        }
    }
    if output != kept.len() || output == input_count {
        return Err(category(
            "retained member deletion event is absent or incomplete",
            Span::default(),
        ));
    }
    Ok(())
}

impl Materializer<'_, '_> {
    pub(super) fn record_member_retention(
        &mut self,
        module: ModuleId,
        host: &CtSymbolId,
        input_count: usize,
        kept: &[usize],
    ) -> CompileResult<bool> {
        if module != host.module {
            return Err(category(
                "retained member host module differs",
                Span::default(),
            ));
        }
        if !self.declaration_lineage_preserved
            || self.generated_context
            || original_host(self.parsed, host, &mut self.environment.budget)?.is_none()
        {
            return Ok(false);
        }
        self.environment
            .budget
            .reserve_analysis_work(kept.len() as u64 + self.member_retentions.len() as u64 + 1)?;
        self.environment
            .budget
            .reserve_generated_nodes(kept.len() as u64 + 1)?;
        self.environment.budget.reserve_temporary_bytes(
            kept.len() as u64 * std::mem::size_of::<u32>() as u64
                + (self.member_retentions.len() as u64 + 1)
                    * std::mem::size_of::<MemberRetention>() as u64,
        )?;
        let mut positions = Vec::with_capacity(kept.len());
        for ordinal in kept {
            positions.push(
                u32::try_from(*ordinal)
                    .map_err(|_| category("retained member ordinal overflow", Span::default()))?,
            );
        }
        let map = RetainedMemberOrigins {
            original_count: u32::try_from(input_count)
                .map_err(|_| category("retained member count overflow", Span::default()))?,
            positions,
        };
        validate_events(
            self.parsed,
            &self.environment.ct_only,
            host,
            input_count,
            kept,
            &map,
            &mut self.environment.budget,
        )?;
        let old = self.member_retentions.capacity();
        self.member_retentions.reserve_exact(1);
        self.environment
            .budget
            .release_temporary_bytes(old as u64 * std::mem::size_of::<MemberRetention>() as u64);
        self.member_retentions.push(MemberRetention {
            host: host.clone(),
            map,
        });
        Ok(true)
    }

    pub(super) fn attach_member_retentions(
        &mut self,
        module: ModuleId,
        rows: &mut [DeclarationOrigin],
    ) -> CompileResult<()> {
        let events = std::mem::take(&mut self.member_retentions);
        let capacity = events.capacity();
        for event in events {
            self.environment.budget.reserve_analysis_work(1)?;
            if event.host.module != module {
                return Err(category(
                    "retained member output module differs",
                    Span::default(),
                ));
            }
            let row = rows
                .get_mut(event.host.declaration.0 as usize)
                .ok_or_else(|| {
                    category("retained member output host row absent", Span::default())
                })?;
            if row.declaration != event.host.declaration || row.retained_members.is_some() {
                return Err(category(
                    "retained member output host origin differs or repeats",
                    Span::default(),
                ));
            }
            self.environment
                .validate_original_host_origin(&event.host, row)?;
            row.retained_members = Some(event.map);
        }
        self.environment.budget.release_temporary_bytes(
            capacity as u64 * std::mem::size_of::<MemberRetention>() as u64,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_expand::source_context::{
        DeclarationPlacement, SourceIdentityContext, capture_original,
    };

    #[test]
    fn g443_actual_source_deletions_bind_original_events_and_host() {
        let gone = |name| format!("fn {name}<T:Copy,#N:i32,life a>(marker:Type)->i32{{return N}}");
        let keep = |name| format!("fn {name}()->i32{{return 42}}");
        for (methods, expected) in [
            (format!("{}{}", gone("a"), keep("b")), vec![1usize]),
            (format!("{}{}", keep("a"), gone("b")), vec![0]),
            (
                format!("{}{}{}", keep("a"), gone("b"), keep("c")),
                vec![0, 2],
            ),
            (format!("{}{}", gone("a"), gone("b")), vec![]),
        ] {
            let text = format!(
                "struct Anchor<life a>{{n:i32}}fn removed()->Type{{return i32}}struct Maker{{{methods}}}struct Other{{fn same()->i32{{return 0}}}}fn main()->i32{{return 0}}"
            );
            let sources = vec![SourceUnit::new(
                zil_base::FileId(0),
                ModuleId(0),
                "member-events.zil",
                text,
            )];
            let parsed = vec![Arc::new(zil_syntax::parse(&sources[0]))];
            assert!(parsed[0].diagnostics.is_empty());
            let syntax = vec![&parsed[0].module];
            let paths = BTreeMap::new();
            let imports = vec![vec![]];
            let limits = Limits::default();
            let target = TargetProfile::host_llvm();
            let (original_rows, _) = capture_original(
                &sources,
                &syntax,
                SourceIdentityContext::ClosedSnapshot {
                    root: ModuleId(0),
                    aliases: &paths,
                    declarations: DeclarationPlacement::Original,
                },
                &limits,
            )
            .unwrap();
            let original_rows = original_rows.unwrap();
            let mut environment = Environment::new(
                &sources,
                &parsed,
                &paths,
                &imports,
                ModuleId(0),
                sources[0].fingerprint(),
                &target,
                &limits,
            )
            .unwrap();
            environment
                .bind_source_origins(Some(&original_rows))
                .unwrap();
            environment.classify_definitions().unwrap();
            let host = CtSymbolId {
                module: ModuleId(0),
                declaration: zil_base::NodeId(2),
                generation_path: None,
            };
            let s::ItemKind::Aggregate(original) = &parsed[0].module.items[2].kind else {
                unreachable!()
            };
            let valid = RetainedMemberOrigins {
                original_count: original.methods.len() as u32,
                positions: expected.iter().map(|n| *n as u32).collect(),
            };
            validate_events(
                &parsed,
                &environment.ct_only,
                &host,
                original.methods.len(),
                &expected,
                &valid,
                &mut environment.budget,
            )
            .unwrap();
            let mut forged = valid.clone();
            forged.original_count += 1;
            assert!(
                validate_events(
                    &parsed,
                    &environment.ct_only,
                    &host,
                    original.methods.len(),
                    &expected,
                    &forged,
                    &mut environment.budget
                )
                .is_err()
            );
            if expected.as_slice() == [1] {
                let mut forged = valid.clone();
                forged.positions[0] = 0; // Valid shape and range, false event.
                assert!(
                    validate_events(
                        &parsed,
                        &environment.ct_only,
                        &host,
                        original.methods.len(),
                        &expected,
                        &forged,
                        &mut environment.budget
                    )
                    .is_err()
                );
            }
            environment
                .validate_original_host_origin(&host, &original_rows[0].items[2])
                .unwrap();
            assert!(
                environment
                    .validate_original_host_origin(&host, &original_rows[0].items[3])
                    .is_err()
            );
            let mut swapped_identity = original_rows[0].items[2].clone();
            swapped_identity.identity = original_rows[0].items[3].identity.clone();
            assert!(
                environment
                    .validate_original_host_origin(&host, &swapped_identity)
                    .is_err()
            );

            let output = materialize(
                &sources,
                &parsed,
                &syntax,
                &paths,
                &imports,
                ModuleId(0),
                sources[0].fingerprint(),
                &target,
                &limits,
                None,
                true,
            )
            .unwrap();
            let rows = output.declaration_origins.unwrap();
            let maker = &rows[0].items[1]; // removed top-level CT-only helper.
            assert_eq!(maker.declaration, zil_base::NodeId(2));
            assert_eq!(maker.retained_members.as_ref(), Some(&valid));
            let s::ItemKind::Aggregate(actual) = &output.modules[0].items[1].kind else {
                unreachable!()
            };
            assert_eq!(actual.methods.len(), expected.len());
            assert_eq!(actual.name, "Maker", "all-methods-deleted host must remain");
        }
    }
}
