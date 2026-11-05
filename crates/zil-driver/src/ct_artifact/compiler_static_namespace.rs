//! Definition-only checking of every member in one actual frozen CT namespace.
//! The caller authenticates source/use correspondence; no concrete instance can
//! grant an abstract parameter arithmetic, fields, or runtime capabilities.
use super::*;

pub(crate) fn check<'a>(
    modules: &'a [SourceModule<'a>],
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
    host: &'a s::Item,
    module: ModuleId,
    limits: &mut Limits,
) -> R<()> {
    let mut compiler = Compiler::unindexed(modules, limits);
    compiler.generated_catalog = catalog;
    let result = compiler
        .index()
        .and_then(|()| compiler.index_generated())
        .and_then(|()| compiler.check_namespace(module, host));
    // Definition work remains spent on failure. No symbolic output is retained.
    let remaining = compiler.budget.declared();
    drop(compiler);
    limits.max_analysis_steps = remaining.max_analysis_work as usize;
    limits.max_nodes = remaining.max_generated_nodes as usize;
    limits.max_source_bytes = remaining.max_temporary_bytes as usize;
    limits.max_functions = remaining.max_instances as usize;
    result
}

impl<'a> Compiler<'a> {
    fn check_namespace(&mut self, module: ModuleId, host: &'a s::Item) -> R<()> {
        let owner = self.module_index(module, host.span)?;
        // Pointer selection only relates the borrowed node to this immutable
        // inventory. item_symbol rederives the actual original/generated ID.
        self.item_symbol(owner, host)?;
        let s::ItemKind::Aggregate(aggregate) = &host.kind else {
            return Err(forbidden(host.span, "CT namespace source is not a struct"));
        };
        if aggregate.kind != s::AggregateKind::Struct || !host.attributes.is_empty() {
            return Err(forbidden(
                host.span,
                "CT namespace requires an ordinary struct without attributes",
            ));
        }
        let first = self.declarations.len();
        // Register the entire actual sibling inventory before checking any
        // body. Scratch compilers must see exactly the same declaration IDs.
        for (ordinal, item) in aggregate.methods.iter().enumerate() {
            self.budget.node(item.span)?;
            let s::ItemKind::Function(function) = &item.kind else {
                return Err(forbidden(
                    item.span,
                    "CT namespace contains a non-function member",
                ));
            };
            if !item.attributes.is_empty()
                || !function.contract.is_empty()
                || !function.where_clause.is_empty()
                || function.params.iter().any(|p| !p.attributes.is_empty())
                || matches!(function.body, s::FunctionBody::Declaration)
            {
                return Err(forbidden(
                    item.span,
                    "CT namespace member attributes or contracts require their consumer",
                ));
            }
            for parameter in &function.params {
                self.budget.text(&parameter.name, parameter.span)?;
                if parameter.name == "self" {
                    return Err(forbidden(
                        parameter.span,
                        "CT namespace member cannot have a runtime receiver",
                    ));
                }
            }
            for earlier in &aggregate.methods[..ordinal] {
                self.budget.node(earlier.span)?;
                if let s::ItemKind::Function(other) = &earlier.kind {
                    self.budget
                        .work(function.name.len().min(other.name.len()) + 1, item.span)?;
                    if other.name == function.name {
                        return Err(error(item.span, "duplicate CT namespace member"));
                    }
                }
            }
            self.budget
                .work(self.declarations.len() + self.member_hosts.len(), item.span)?;
            if self.member_hosts.iter().any(|row| {
                std::ptr::eq(row.host, host)
                    && std::ptr::eq(self.declarations[row.declaration].item, item)
            }) {
                continue;
            }
            Budget::take(
                &mut self.budget.bytes,
                (self.declarations.len() + 1)
                    .saturating_mul(std::mem::size_of::<Declaration>())
                    .saturating_add(
                        (self.member_hosts.len() + 1)
                            .saturating_mul(std::mem::size_of::<members::MemberLocation>()),
                    ),
                item.span,
                "CT namespace complete directory backing exhausted",
            )?;
            self.declarations.reserve_exact(1);
            self.member_hosts.reserve_exact(1);
            self.member_hosts.push(members::MemberLocation {
                declaration: self.declarations.len(),
                host,
            });
            self.declarations.push(Declaration {
                module: owner,
                item,
                function,
            });
        }
        // Include any eagerly indexed CT-only members of this same host.
        for declaration in 0..self.declarations.len() {
            self.budget.node(host.span)?;
            let belongs = self
                .member_hosts
                .iter()
                .any(|row| row.declaration == declaration && std::ptr::eq(row.host, host));
            self.budget.work(self.member_hosts.len(), host.span)?;
            if belongs {
                self.ensure_generic_definition(declaration)?;
            } else if declaration >= first {
                return Err(error(
                    host.span,
                    "CT namespace directory lost its actual host",
                ));
            }
        }
        Ok(())
    }
}
