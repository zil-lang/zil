//! Original struct namespaces: CT-only definitions are indexed eagerly; ordinary
//! pure static members enter the actual CT dependency graph on demand.
//! Names select an AST binding; only its original host/member nodes name rows.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct MemberLocation<'a> {
    pub declaration: usize,
    pub(super) host: &'a s::Item,
}

impl<'a> Compiler<'a> {
    pub(super) fn index_members(&mut self, module: usize, host: &'a s::Item) -> R<()> {
        let s::ItemKind::Aggregate(aggregate) = &host.kind else {
            return Ok(());
        };
        if aggregate.kind != s::AggregateKind::Struct {
            return Ok(());
        }
        for item in &aggregate.methods {
            self.budget.work(1, item.span)?;
            let s::ItemKind::Function(function) = &item.kind else {
                continue;
            };
            self.budget.work(function.generics.len(), item.span)?;
            let mut receiver = false;
            for parameter in &function.params {
                self.budget
                    .work(parameter.name.len().min(4) + 1, parameter.span)?;
                receiver |= parameter.name == "self";
            }
            if receiver || !self.ct_only_function(module, function)? {
                continue;
            }
            for sibling in &aggregate.methods {
                self.budget.work(1, sibling.span)?;
                if let s::ItemKind::Function(other) = &sibling.kind {
                    self.budget
                        .work(function.name.len().min(other.name.len()) + 1, sibling.span)?;
                    if !std::ptr::eq(sibling, item) && other.name == function.name {
                        return Err(error(sibling.span, "duplicate CT member declaration"));
                    }
                }
            }
            self.budget.node(item.span)?;
            Budget::take(
                &mut self.budget.bytes,
                std::mem::size_of::<Declaration>() + std::mem::size_of::<MemberLocation>(),
                item.span,
                "CT original member inventory bytes exhausted",
            )?;
            self.budget.work(
                self.declarations
                    .len()
                    .saturating_add(self.member_hosts.len()),
                item.span,
            )?;
            self.declarations.reserve_exact(1);
            self.member_hosts.reserve_exact(1);
            self.member_hosts.push(MemberLocation {
                declaration: self.declarations.len(),
                host,
            });
            self.declarations.push(Declaration {
                module,
                item,
                function,
            });
        }
        Ok(())
    }

    pub(super) fn copy_member_locations(&mut self, other: &mut Self, span: Span) -> R<()> {
        if self.member_hosts.is_empty() {
            return Ok(());
        }
        self.budget.work(self.member_hosts.len(), span)?;
        Budget::take(
            &mut self.budget.bytes,
            self.member_hosts.len().saturating_mul(
                std::mem::size_of::<MemberLocation>() + std::mem::size_of::<Declaration>(),
            ),
            span,
            "CT original member declaration copy bytes exhausted",
        )?;
        other.member_hosts.extend_from_slice(&self.member_hosts);
        Ok(())
    }

    pub(super) fn member_function(
        &mut self,
        current: usize,
        path: &[String],
        span: Span,
    ) -> R<Option<usize>> {
        if !(2..=3).contains(&path.len()) {
            return Ok(None);
        }
        let (module, host_name, member_name) = if path.len() == 2 {
            for import in self.modules[current].imports {
                self.budget
                    .work(path[0].len().min(import.alias.len()) + 1, span)?;
                if import.alias == path[0] {
                    return Ok(None);
                }
            }
            (current, &path[0], &path[1])
        } else {
            let mut selected = None;
            for import in self.modules[current].imports {
                self.budget
                    .work(path[0].len().min(import.alias.len()) + 1, span)?;
                if import.alias == path[0] {
                    let ImportTarget::Module(id) = import.target else {
                        return Ok(None);
                    };
                    selected = Some(self.module_index(id, span)?);
                }
            }
            let Some(module) = selected else {
                return Ok(None);
            };
            (module, &path[1], &path[2])
        };
        let mut host = self.generated_named_item(module, host_name, span)?;
        for item in &self.modules[module].syntax.items {
            self.budget.work(1, span)?;
            let s::ItemKind::Aggregate(aggregate) = &item.kind else {
                continue;
            };
            self.budget
                .work(aggregate.name.len().min(host_name.len()) + 1, span)?;
            if aggregate.name == *host_name && host.replace(item).is_some() {
                return Err(error(span, "duplicate CT host declaration"));
            }
        }
        let Some(host) = host else { return Ok(None) };
        let s::ItemKind::Aggregate(aggregate) = &host.kind else {
            unreachable!()
        };
        let mut selected = None;
        for member in &aggregate.methods {
            self.budget.work(1, span)?;
            let s::ItemKind::Function(function) = &member.kind else {
                continue;
            };
            self.budget
                .work(function.name.len().min(member_name.len()) + 1, span)?;
            if function.name == *member_name && selected.replace(member).is_some() {
                return Err(error(span, "duplicate CT member declaration"));
            }
        }
        let Some(item) = selected else {
            return Err(error(span, "unknown CT static member"));
        };
        if module != current && (!host.public || !item.public) {
            return Err(error(
                span,
                "CT member or its host is private to its defining module",
            ));
        }
        for location in &self.member_hosts {
            self.budget.work(1, span)?;
            let declaration = self.declarations[location.declaration];
            if declaration.module == module
                && std::ptr::eq(location.host, host)
                && std::ptr::eq(declaration.item, item)
            {
                return Ok(Some(location.declaration));
            }
        }
        // Directory selection grants no CT effects or Copy capability. The
        // ordinary function is checked by the same actual dependency compiler
        // used for a top-level function; it is never reclassified CT-only.
        if aggregate.kind != s::AggregateKind::Struct {
            return Err(forbidden(
                span,
                "CT static member requires its actual ordinary struct host",
            ));
        }
        let s::ItemKind::Function(function) = &item.kind else {
            unreachable!()
        };
        for parameter in &function.params {
            self.budget.work(parameter.name.len().min(4) + 1, span)?;
            if parameter.name == "self" {
                return Err(forbidden(
                    span,
                    "CT static member cannot consume a runtime receiver",
                ));
            }
        }
        self.budget.node(span)?;
        self.budget.work(
            self.declarations
                .len()
                .saturating_add(self.member_hosts.len()),
            span,
        )?;
        Budget::take(
            &mut self.budget.bytes,
            self.declarations
                .len()
                .saturating_add(1)
                .saturating_mul(std::mem::size_of::<Declaration>())
                .saturating_add(
                    self.member_hosts
                        .len()
                        .saturating_add(1)
                        .saturating_mul(std::mem::size_of::<MemberLocation>()),
                ),
            span,
            "CT actual ordinary member directory backing exhausted",
        )?;
        self.declarations.reserve_exact(1);
        self.member_hosts.reserve_exact(1);
        let declaration = self.declarations.len();
        self.member_hosts.push(MemberLocation { declaration, host });
        self.declarations.push(Declaration {
            module,
            item,
            function,
        });
        Ok(Some(declaration))
    }
}
