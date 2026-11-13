//! Borrowed preflight for the AST trees specialization may clone or substitute.
//! This is a private bound on work, never a type or capability certificate.
use super::*;

pub(crate) struct Budget<'a> {
    pub(crate) target_pointer_width: u16,
    pub(crate) context: origins::WalkContext,
    limits: &'a Limits,
    nodes: usize,
    work: usize,
    temporary_bytes: Option<usize>,
    lineage_bytes: usize,
    scoped_scratch: usize,
    enum_profiles: BTreeSet<Span>,
    enum_header: Option<Span>,
    primitive_binders: bool,
    anonymous_sums: bool,
    borrowed_keys: bool,
    type_member_patterns: bool,
    reference_patterns: bool,
    generic_methods: bool,
    dynamic_interfaces: bool,
    stored_lifetimes: bool,
    source_identity: bool,
    shared_enum_addresses: bool,
    task_result_worker: bool,
    static_task_create: bool,
}
impl<'a> Budget<'a> {
    pub(crate) fn new(limits: &'a Limits) -> Self {
        Self {
            target_pointer_width: 64,
            context: origins::WalkContext::default(),
            limits,
            nodes: limits.max_nodes,
            work: limits.max_analysis_steps,
            temporary_bytes: None,
            lineage_bytes: 0,
            scoped_scratch: 0,
            enum_profiles: BTreeSet::new(),
            enum_header: None,
            primitive_binders: false,
            anonymous_sums: false,
            borrowed_keys: false,
            type_member_patterns: false,
            reference_patterns: false,
            generic_methods: false,
            dynamic_interfaces: false,
            stored_lifetimes: false,
            source_identity: false,
            shared_enum_addresses: false,
            task_result_worker: false,
            static_task_create: false,
        }
    }
    pub(crate) fn has_dynamic(&self) -> bool {
        self.dynamic_interfaces
    }
    pub(crate) fn has_stored_lifetimes(&self) -> bool {
        self.stored_lifetimes
    }
    pub(crate) fn note_stored_lifetimes(&mut self) {
        self.stored_lifetimes = true;
    }
    pub(crate) fn note_dynamic(&mut self) {
        self.dynamic_interfaces = true;
    }
    pub(crate) fn needs_source_identity(&self) -> bool {
        self.dynamic_interfaces || self.source_identity
    }
    pub(crate) fn note_source_identity(&mut self) {
        self.source_identity = true;
    }
    fn task_result_inventory(&mut self) {
        if self.task_result_worker && self.static_task_create {
            self.note_source_identity();
            self.note_stored_lifetimes();
        }
    }
    /// Record conservative inventory demand during an already paid signature
    /// walk. Actual static-worker identity and result sources are checked later.
    pub(crate) fn note_task_result_worker(&mut self, function: &Function) {
        if function.return_type.as_ref().is_some_and(|result| {
            matches!(
                grouped(result).kind,
                TypeKind::Reference { mutable: false, .. }
            )
        }) {
            self.task_result_worker = true;
            self.task_result_inventory();
        }
    }
    pub(crate) fn note_static_task_create(&mut self, expression: &Expr) {
        fn static_path(expression: &Expr) -> bool {
            match &expression.kind {
                ExprKind::Name(_) => true,
                ExprKind::Group(inner) | ExprKind::TypeApply { callee: inner, .. } => {
                    static_path(inner)
                }
                ExprKind::Member { object, .. } => static_path(object),
                _ => false,
            }
        }
        if let ExprKind::Call { callee, arguments } = &expression.kind
            && crate::foreign::bare_name(callee) == Some("task_create")
            && arguments.first().is_some_and(|a| static_path(&a.value))
        {
            self.static_task_create = true;
            self.task_result_inventory();
        }
    }
    pub(crate) fn has_shared_enum_addresses(&self) -> bool {
        self.shared_enum_addresses
    }
    pub(crate) fn note_shared_enum_addresses(&mut self) {
        self.shared_enum_addresses = true;
    }
    pub(crate) fn remaining_limits(&self) -> Limits {
        Limits {
            max_nodes: self.nodes,
            max_analysis_steps: self.work,
            max_source_bytes: self
                .temporary_bytes
                .unwrap_or(self.limits.max_source_bytes)
                .saturating_sub(self.lineage_bytes),
            ..self.limits.clone()
        }
    }
    /// The CT planning caller supplies its current remaining temporary quota.
    /// Every prepaid syntax/key visit bounds the allocation it can authorize;
    /// ordinary expansion keeps its existing node/work-only ledger.
    pub(crate) fn limit_temporary_bytes(&mut self, bytes: usize) {
        self.temporary_bytes = Some(bytes.saturating_add(self.lineage_bytes));
    }
    /// A borrowed visit can use the same conservative AST traversal upper
    /// bounds as a clone without retaining its temporary stack forever. The
    /// callback returns no owned syntax; its temporary state is gone on return.
    pub(crate) fn borrowed_work(
        &mut self,
        span: Span,
        run: impl FnOnce(&mut Self) -> R<()>,
    ) -> R<()> {
        self.allocation_scope(span, false, 1024, run)
    }
    /// Prepay a real retained clone. Unlike a read-only lookup, every AST/text
    /// copy upper bound visited by this callback remains charged afterwards.
    pub(crate) fn retained_copy<T>(
        &mut self,
        span: Span,
        run: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        self.allocation_scope(span, true, 1024, run)
    }
    fn allocation_scope<T>(
        &mut self,
        span: Span,
        retained: bool,
        scratch: usize,
        run: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        let previous = self.temporary_bytes;
        let previous_scratch = self.scoped_scratch;
        let pool = self
            .remaining_limits()
            .max_source_bytes
            .checked_add(self.lineage_bytes)
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "temporary allocation pool overflow",
                    Some(span),
                )
            })?;
        self.temporary_bytes = Some(pool);
        self.scoped_scratch = 0;
        let result = self.scratch_storage(scratch, span).and_then(|()| run(self));
        let consumed = pool.saturating_sub(self.temporary_bytes.unwrap_or(pool));
        let scratch = self.scoped_scratch;
        self.temporary_bytes = previous;
        self.scoped_scratch = previous_scratch;
        if retained && result.is_ok() {
            self.lineage_bytes = self
                .lineage_bytes
                .checked_add(consumed.saturating_sub(scratch))
                .ok_or_else(|| {
                    Diagnostic::limit(Phase::Expand, "retained copy bytes overflow", Some(span))
                })?;
        }
        result
    }
    /// A real temporary allocation inside allocation_scope. It is released
    /// only after the callback's structural traversal has returned.
    pub(crate) fn scratch_storage(&mut self, bytes: usize, span: Span) -> R<()> {
        if self.temporary_bytes.is_none() {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "temporary storage has no active allocation scope",
                Some(span),
            ));
        }
        self.temporary(bytes, span)?;
        self.scoped_scratch = self.scoped_scratch.checked_add(bytes).ok_or_else(|| {
            Diagnostic::limit(Phase::Expand, "temporary storage size overflow", Some(span))
        })?;
        Ok(())
    }
    /// Reserve the new allocation while the old Vec allocation is still live.
    /// Only successful realloc/transfer releases the old allocation's charge.
    pub(crate) fn lineage_push<T>(&mut self, values: &mut Vec<T>, value: T, span: Span) -> R<()> {
        self.node(span, 0)?;
        if values.len() == values.capacity() {
            let old = values
                .capacity()
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| {
                    Diagnostic::limit(Phase::Expand, "lineage vector size overflow", Some(span))
                })?;
            let growth = values.capacity().max(4);
            let new = values
                .capacity()
                .checked_add(growth)
                .and_then(|capacity| capacity.checked_mul(std::mem::size_of::<T>()))
                .ok_or_else(|| {
                    Diagnostic::limit(Phase::Expand, "lineage vector growth overflow", Some(span))
                })?;
            self.identity_work(values.len(), span)?;
            self.lineage_bytes(new, span)?;
            if values.try_reserve_exact(growth).is_err() {
                // reserve_exact's error leaves the original allocation intact.
                self.lineage_bytes -= new;
                return Err(Diagnostic::limit(
                    Phase::Expand,
                    "lineage vector allocation failed",
                    Some(span),
                ));
            }
            self.lineage_bytes = self.lineage_bytes.checked_sub(old).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "lineage vector release exceeds its retained allocation",
                    Some(span),
                )
            })?;
        }
        values.push(value);
        Ok(())
    }
    pub(crate) fn drop_lineage_vec<T>(&mut self, values: Vec<T>, span: Span) -> R<()> {
        let bytes = values
            .capacity()
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "lineage vector retirement overflow",
                    Some(span),
                )
            })?;
        drop(values);
        self.lineage_bytes = self.lineage_bytes.checked_sub(bytes).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "lineage vector retirement exceeds its allocation",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn lineage_checkpoint(&self) -> usize {
        self.lineage_bytes
    }
    /// All allocations made since this checkpoint must belong to value; the
    /// caller must not commit a different output between the mark and discard.
    pub(crate) fn discard_lineage<T>(&mut self, checkpoint: usize, value: T, span: Span) -> R<()> {
        drop(value);
        if checkpoint > self.lineage_bytes {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "lineage retirement has no matching allocation checkpoint",
                Some(span),
            ));
        }
        self.lineage_bytes = checkpoint;
        Ok(())
    }
    fn temporary(&mut self, bytes: usize, span: Span) -> R<()> {
        if let Some(left) = &mut self.temporary_bytes {
            if bytes > left.saturating_sub(self.lineage_bytes) {
                return Err(Diagnostic::limit(
                    Phase::Expand,
                    "declaration lineage temporary bytes exhausted",
                    Some(span),
                ));
            }
            *left = left.checked_sub(bytes).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "associated constant planning temporary bytes exhausted",
                    Some(span),
                )
            })?;
        }
        Ok(())
    }
    pub(crate) fn adopt_remaining(&mut self, remaining: &Limits) {
        self.nodes = self.nodes.min(remaining.max_nodes);
        self.work = self.work.min(remaining.max_analysis_steps);
        if let Some(bytes) = &mut self.temporary_bytes {
            *bytes = (*bytes).min(
                remaining
                    .max_source_bytes
                    .saturating_add(self.lineage_bytes),
            );
        } else {
            // A child may retain its first allocation even when this caller
            // has not allocated lineage yet. Never discard that debit.
            self.lineage_bytes = self.lineage_bytes.max(
                self.limits
                    .max_source_bytes
                    .saturating_sub(remaining.max_source_bytes),
            );
        }
    }
    /// New declaration headers and member vectors are real allocations even
    /// when the older AST planner uses only its node/work ledger. Keep that
    /// marginal allocation quota separate instead of enabling unrelated AST
    /// temporary charges for every old node visit.
    pub(crate) fn lineage_bytes(&mut self, bytes: usize, span: Span) -> R<()> {
        self.work = self.work.checked_sub(bytes.max(1)).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "declaration lineage copy work exhausted",
                Some(span),
            )
        })?;
        if bytes > self.remaining_limits().max_source_bytes {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "declaration lineage bytes exhausted",
                Some(span),
            ));
        }
        self.lineage_bytes = self.lineage_bytes.checked_add(bytes).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "declaration lineage bytes overflow",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn note_enum_profile(&mut self, span: Span) -> R<()> {
        self.node(span, 0)?;
        self.enum_profiles.insert(span);
        Ok(())
    }
    pub(crate) fn has_enum_profiles(&self) -> bool {
        !self.enum_profiles.is_empty()
    }
    pub(crate) fn note_type_binder(&mut self, name: &str, span: Span) -> R<()> {
        if super::super::integer_name(name)
            || matches!(name, "bool" | "void" | "c_handle" | "c_wstr")
        {
            self.node(span, 0)?;
            self.primitive_binders = true;
        }
        Ok(())
    }
    pub(crate) fn has_borrowed_keys(&self) -> bool {
        self.borrowed_keys
    }
    pub(crate) fn note_borrowed_keys(&mut self) {
        self.borrowed_keys = true;
    }
    pub(crate) fn has_anonymous_sums(&self) -> bool {
        self.anonymous_sums
    }
    pub(crate) fn has_reference_patterns(&self) -> bool {
        self.reference_patterns
    }
    pub(crate) fn has_type_member_patterns(&self) -> bool {
        self.type_member_patterns
    }
    pub(crate) fn has_primitive_binders(&self) -> bool {
        self.primitive_binders
    }
    pub(super) fn enum_profile(&self, span: Span) -> bool {
        self.enum_profiles.contains(&span)
    }
    pub(crate) fn node(&mut self, span: Span, depth: usize) -> R<()> {
        let node_bytes = std::mem::size_of::<Item>()
            .saturating_add(std::mem::size_of::<Expr>())
            .saturating_add(std::mem::size_of::<TypeRef>())
            .saturating_add(std::mem::size_of::<Function>());
        self.temporary(node_bytes, span)?;
        if depth > self.limits.max_call_depth || self.nodes == 0 || self.work == 0 {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "generic expansion budget exceeded",
                Some(span),
            ));
        }
        self.nodes -= 1;
        self.work -= 1;
        Ok(())
    }
    pub(crate) fn numeric_work(&mut self, amount: usize, span: Span) -> R<()> {
        self.temporary(amount, span)?;
        self.work = self.work.checked_sub(amount).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "floating literal conversion work exhausted",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn identity_node(&mut self, depth: usize, span: Span) -> R<()> {
        self.identity_work(1, span)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "structural type key nesting exhausted",
                Some(span),
            ));
        }
        Ok(())
    }
    pub(crate) fn identity_work(&mut self, amount: usize, span: Span) -> R<()> {
        self.temporary(amount, span)?;
        self.work = self.work.checked_sub(amount).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "structural type identity work exhausted",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn text(&mut self, text: &str, span: Span) -> R<()> {
        self.temporary(
            text.len().saturating_add(4 * std::mem::size_of::<String>()),
            span,
        )?;
        self.work = self.work.checked_sub(text.len()).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "generic metadata budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn wide_units(&mut self, units: &[u16], span: Span) -> R<()> {
        self.work = self.work.checked_sub(units.len()).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "generic static UTF-16 budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn ty(&mut self, ty: &TypeRef) -> R<()> {
        self.ty_at(ty, 0, None)
    }
    pub(crate) fn const_expression(&mut self, expression: &Expr) -> R<()> {
        self.expr(expression, 0, None)
    }
    pub(crate) fn const_expression_depth(&mut self, expression: &Expr, depth: usize) -> R<()> {
        self.expr(expression, depth, None)
    }

    pub(crate) fn const_function(&mut self, function: &Function) -> R<()> {
        self.function(function, 0, None)
    }
    pub(crate) fn ty_depth(&mut self, ty: &TypeRef, depth: usize) -> R<()> {
        self.ty_at(ty, depth, None)
    }
    pub(crate) fn substituted_type(
        &mut self,
        ty: &TypeRef,
        substitutions: &BTreeMap<String, TypeRef>,
    ) -> R<()> {
        self.ty_at(ty, 0, Some(substitutions))
    }
    pub(crate) fn substituted_type_depth(
        &mut self,
        ty: &TypeRef,
        substitutions: &BTreeMap<String, TypeRef>,
        depth: usize,
    ) -> R<()> {
        self.ty_at(ty, depth, Some(substitutions))
    }
    pub(crate) fn item(&mut self, item: &Item) -> R<()> {
        self.item_at(item, 0, None)
    }
    pub(crate) fn function_snapshot(&mut self, function: &Function) -> R<()> {
        self.function(function, 0, None)
    }
    pub(crate) fn generic_parameters_snapshot(&mut self, parameters: &[GenericParam]) -> R<()> {
        self.generics(parameters, 0, None, false)
    }
    pub(crate) fn substituted_item(
        &mut self,
        item: &Item,
        substitutions: &BTreeMap<String, TypeRef>,
    ) -> R<()> {
        self.item_at(item, 0, Some(substitutions))
    }
    fn ty_at(
        &mut self,
        ty: &TypeRef,
        depth: usize,
        substitutions: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.ty_context(ty, depth, substitutions, false)
    }
    fn associated_type_substitution(
        &mut self,
        ty: &TypeRef,
        substitutions: &BTreeMap<String, TypeRef>,
        depth: usize,
        slice_element: bool,
    ) -> R<bool> {
        self.ty(ty)?;
        let key = super::trait_bounds::associated::equation_key(ty, self)?;
        // A later Substitution walk repeats the lookup without a budget.
        if self.has_anonymous_sums() {
            super::type_keys::prepay(ty, self)?;
            self.identity_work(11, ty.span)?;
        }
        self.text(&key, ty.span)?;
        if let Some(value) = substitutions.get(&key) {
            self.ty_context(value, depth, None, slice_element)?;
            return Ok(true);
        }
        Ok(false)
    }
    fn ty_context(
        &mut self,
        ty: &TypeRef,
        depth: usize,
        substitutions: Option<&BTreeMap<String, TypeRef>>,
        slice_element: bool,
    ) -> R<()> {
        self.node(ty.span, depth)?;
        if let Some(substitutions) = substitutions
            && super::trait_bounds::associated::parts(ty).is_some()
            && self.associated_type_substitution(ty, substitutions, depth, slice_element)?
        {
            return Ok(());
        }
        if let TypeKind::Named { path, args } = &ty.kind
            && args.is_empty()
            && let Some(substitutions) = substitutions
        {
            if path.len() != 1 {
                for part in path {
                    self.text(part, ty.span)?;
                    self.node(ty.span, depth)?;
                }
            }
            if let Some(value) = substitutions.get(super::substitution_name(path).as_ref()) {
                // A substitution is simultaneous. Do not substitute caller T again
                // merely because the callee's type parameter is also named T.
                return self.ty_context(value, depth, None, slice_element);
            }
        }
        if matches!(ty.kind, TypeKind::Dyn { .. }) {
            self.dynamic_interfaces = true;
        }
        if let TypeKind::Dyn { bindings, .. } = &ty.kind {
            for binding in bindings {
                self.node(binding.span, depth + 1)?;
                match &binding.kind {
                    DynBindingKind::Type { member, value } => {
                        for name in member {
                            self.text(name, binding.span)?;
                        }
                        self.ty_context(value, depth + 2, substitutions, false)?;
                    }
                    DynBindingKind::Constant { member, ty, value } => {
                        for name in member {
                            self.text(name, binding.span)?;
                        }
                        self.ty_context(ty, depth + 2, substitutions, false)?;
                        self.expr(value, depth + 2, substitutions)?;
                    }
                    DynBindingKind::Method { member, arguments } => {
                        for name in member {
                            self.text(name, binding.span)?;
                        }
                        for actual in arguments {
                            self.arg(actual, depth + 2, substitutions)?;
                        }
                    }
                }
            }
        }
        if let TypeKind::ForeignFunction { abi, .. } = &ty.kind {
            self.text(abi, ty.span)?;
        }
        if let TypeKind::Named { path, .. } = &ty.kind
            && path.len() > 1
            && let Some(replacement) = substitutions
                .and_then(|s| s.get(&path[0]))
                .and_then(trait_bounds::plain)
        {
            self.text(replacement, ty.span)?;
        }
        match &ty.kind {
            TypeKind::Named { path, args } | TypeKind::Dyn { path, args, .. } => {
                for name in path {
                    self.text(name, ty.span)?;
                }
                for arg in args {
                    // A real shared Address type actual can qualify an ordinary
                    // nominal even when its declaration has only Type formals.
                    // Capture lineage before project flattening; this grants no
                    // permission. The sealed Option keeps its compiler identity.
                    if matches!(&ty.kind, TypeKind::Named { .. })
                        && path.as_slice() != ["Option"]
                        && matches!(arg, GenericArg::Type(t)
                            if matches!(t.kind, TypeKind::Reference {
                                mutable: false, lifetime: Some(_), .. }))
                    {
                        self.note_source_identity();
                        self.shared_enum_addresses = true;
                    }
                    self.arg(arg, depth + 1, substitutions)?;
                }
            }
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(name) = lifetime {
                    self.text(name, ty.span)?;
                }
                self.ty_context(inner, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::Group(inner) | TypeKind::Optional(inner) => {
                self.ty_context(inner, depth + 1, substitutions, slice_element)?
            }
            TypeKind::ForLifetime { lifetime, inner } => {
                self.borrowed_keys = true;
                self.text(lifetime, ty.span)?;
                self.ty_context(inner, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::Union(types) => {
                self.anonymous_sums = true;
                for ty in types {
                    self.ty_context(ty, depth + 1, substitutions, slice_element)?;
                }
            }
            TypeKind::Tuple(types) | TypeKind::Impl(types) => {
                for ty in types {
                    self.ty_context(ty, depth + 1, substitutions, slice_element)?;
                }
            }
            TypeKind::Array { element, length } => {
                if matches!(
                    element.kind,
                    TypeKind::Reference {
                        mutable: false,
                        lifetime: Some(_),
                        ..
                    }
                ) {
                    self.note_stored_lifetimes();
                }
                self.ty_context(
                    element,
                    depth + 1,
                    substitutions,
                    slice_element || length.is_none(),
                )?;
                if let Some(e) = length {
                    self.expr(e, depth + 1, substitutions)?;
                }
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for ty in params {
                    self.ty_context(ty, depth + 1, substitutions, slice_element)?;
                }
                self.ty_context(result, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::Error { error, result } => {
                self.ty_context(error, depth + 1, substitutions, slice_element)?;
                self.ty_context(result, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::MetaEval(e) | TypeKind::MetaSplice(e) => {
                self.expr(e, depth + 1, substitutions)?
            }
            TypeKind::MetaBlock(b) => self.block(b, depth + 1, substitutions)?,
        }
        // Each classified subtree has consumed the existing type budget. Preserve
        // definite non-Copy slice element shapes even in erased aliases and
        // unused templates. Named types/arguments still require ordinary
        // resolution; this does not infer a Copy capability for them.
        if slice_element
            && matches!(
                ty.kind,
                TypeKind::Reference { .. } | TypeKind::Array { length: None, .. }
            )
        {
            return Err(unsupported(
                ty.span,
                "references and unsized arrays cannot be slice elements",
            ));
        }
        // Preserve definite callback formation errors even when specialization
        // or alias elimination later removes this written type. Unresolved
        // names are checked again in their normalized lexical type scope.
        super::super::foreign::callback_type_formation(ty, None)
    }
    fn arg(
        &mut self,
        arg: &GenericArg,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match arg {
            GenericArg::Lifetime(name) => self.text(name, Span::default()),
            GenericArg::Type(t) => self.ty_at(t, depth, subs),
            GenericArg::Const(e) => self.expr(e, depth, subs),
            GenericArg::AssociatedBinding(binding) => {
                self.node(binding.span, depth)?;
                match &binding.kind {
                    DynBindingKind::Type { member, value } => {
                        for name in member {
                            self.text(name, binding.span)?;
                        }
                        self.ty_at(value, depth + 1, subs)
                    }
                    DynBindingKind::Constant { member, ty, value } => {
                        for name in member {
                            self.text(name, binding.span)?;
                        }
                        self.ty_at(ty, depth + 1, subs)?;
                        self.expr(value, depth + 1, subs)
                    }
                    DynBindingKind::Method { .. } => Err(error(
                        binding.span,
                        "a bound equation cannot contain a method slot application",
                    )),
                }
            }
        }
    }
    fn attrs(
        &mut self,
        attrs: &[Attribute],
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        for a in attrs {
            self.node(a.span, depth)?;
            for s in &a.path {
                self.text(s, a.span)?;
            }
            if let Some(s) = &a.string_value {
                self.text(s, a.span)?;
            }
            for e in &a.arguments {
                self.expr(e, depth + 1, subs)?;
            }
        }
        Ok(())
    }
    fn generics(
        &mut self,
        params: &[GenericParam],
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
        nominal: bool,
    ) -> R<()> {
        for p in params {
            self.node(p.span, depth)?;
            self.text(&p.name, p.span)?;
            match &p.kind {
                GenericParamKind::Type(ts) => {
                    self.note_type_binder(&p.name, p.span)?;
                    for t in ts {
                        self.ty_at(t, depth + 1, subs)?;
                        if let Some(owner) = self.enum_header
                            && copy::is_bound(t)
                        {
                            self.note_enum_profile(owner)?;
                        }
                    }
                }
                GenericParamKind::Const(t) => self.ty_at(t, depth + 1, subs)?,
                GenericParamKind::Lifetime => {
                    if nominal {
                        self.note_source_identity();
                    }
                }
            }
        }
        Ok(())
    }
    fn bounds(
        &mut self,
        bounds: &[BoundClause],
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        for b in bounds {
            self.node(b.span, depth)?;
            match &b.subject {
                BoundSubject::Type(t) => self.ty_at(t, depth + 1, subs)?,
                BoundSubject::Lifetime(s) => self.text(s, b.span)?,
            }
            for t in &b.bounds {
                self.ty_at(t, depth + 1, subs)?;
            }
            if let Some(s) = &b.lifetime_bound {
                self.text(s, b.span)?;
            }
        }
        Ok(())
    }
    fn param(
        &mut self,
        p: &Param,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(p.span, depth)?;
        self.text(&p.name, p.span)?;
        self.attrs(&p.attributes, depth + 1, subs)?;
        self.ty_at(&p.ty, depth + 1, subs)
    }
    fn function(
        &mut self,
        f: &Function,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(f.span, depth)?;
        self.text(&f.name, f.span)?;
        self.generics(&f.generics, depth + 1, subs, false)?;
        self.bounds(&f.where_clause, depth + 1, subs)?;
        for clause in &f.contract {
            self.node(clause.span, depth + 1)?;
            self.text(&clause.name, clause.span)?;
            for term in &clause.terms {
                self.contract(term, depth + 2, subs)?;
            }
        }
        for p in &f.params {
            self.param(p, depth + 1, subs)?;
        }
        if let Some(t) = &f.return_type {
            self.ty_at(t, depth + 1, subs)?;
        }
        self.note_task_result_worker(f);
        match &f.body {
            FunctionBody::Block(b) => self.block(b, depth + 1, subs)?,
            FunctionBody::Expression(e) => self.expr(e, depth + 1, subs)?,
            FunctionBody::Declaration => {}
        }
        Ok(())
    }
    fn contract(
        &mut self,
        term: &ContractTerm,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(term.span, depth)?;
        for part in &term.path {
            self.text(part, term.span)?;
        }
        for arg in &term.type_args {
            self.arg(arg, depth + 1, subs)?;
        }
        for arg in &term.arguments {
            self.contract(arg, depth + 1, subs)?;
        }
        if let Some(index) = &term.index {
            self.contract(index, depth + 1, subs)?;
        }
        Ok(())
    }
    fn item_at(
        &mut self,
        item: &Item,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(item.span, depth)?;
        self.attrs(&item.attributes, depth + 1, subs)?;
        match &item.kind {
            ItemKind::TypeAlias(a) => {
                self.text(&a.name, item.span)?;
                self.generics(&a.generics, depth + 1, subs, false)?;
                for t in &a.bounds {
                    self.ty_at(t, depth + 1, subs)?;
                }
                if let Some(t) = &a.value {
                    self.ty_at(t, depth + 1, subs)?;
                }
            }
            ItemKind::Function(f) => self.function(f, depth + 1, subs)?,
            ItemKind::Aggregate(a) => {
                self.text(&a.name, item.span)?;
                self.generics(&a.generics, depth + 1, subs, true)?;
                for f in &a.fields {
                    self.node(f.span, depth + 1)?;
                    self.text(&f.name, f.span)?;
                    self.attrs(&f.attributes, depth + 2, subs)?;
                    self.ty_at(&f.ty, depth + 2, subs)?;
                    if let Some(e) = &f.default {
                        self.expr(e, depth + 2, subs)?;
                    }
                }
                for m in &a.methods {
                    self.item_at(m, depth + 1, subs)?;
                }
            }
            ItemKind::Enum(e) => {
                self.text(&e.name, item.span)?;
                let old_header = self.enum_header.replace(item.span);
                self.generics(&e.generics, depth + 1, subs, true)?;
                self.enum_header = old_header;
                for v in &e.variants {
                    self.node(v.span, depth + 1)?;
                    self.text(&v.name, v.span)?;
                    self.attrs(&v.attributes, depth + 2, subs)?;
                    match &v.fields {
                        VariantFields::Unit => {}
                        VariantFields::Tuple(ts) => {
                            for t in ts {
                                self.ty_at(t, depth + 2, subs)?;
                            }
                        }
                        VariantFields::Record(fs) => {
                            self.note_enum_profile(item.span)?;
                            for f in fs {
                                self.node(f.span, depth + 2)?;
                                self.text(&f.name, f.span)?;
                                self.attrs(&f.attributes, depth + 3, subs)?;
                                self.ty_at(&f.ty, depth + 3, subs)?;
                                if let Some(e) = &f.default {
                                    self.expr(e, depth + 3, subs)?;
                                }
                            }
                        }
                    }
                }
            }
            ItemKind::Impl(i) => {
                // Capture real project module/declaration origins before the
                // generic inherent body is flattened or lifted. A later
                // single-module ledger cannot recover ownership or privacy.
                if i.trait_type.is_none() && !i.generics.is_empty() {
                    self.note_source_identity();
                }
                self.generics(&i.generics, depth + 1, subs, false)?;
                self.bounds(&i.where_clause, depth + 1, subs)?;
                self.ty_at(&i.target, depth + 1, subs)?;
                if let Some(t) = &i.trait_type {
                    self.ty_at(t, depth + 1, subs)?;
                }
                if let Some(p) = &i.receiver_default {
                    self.param(p, depth + 1, subs)?;
                }
                for m in &i.members {
                    self.item_at(m, depth + 1, subs)?;
                }
            }
            ItemKind::Import(i) => {
                for s in &i.path {
                    self.text(s, item.span)?;
                }
                if let Some(s) = &i.alias {
                    self.text(s, item.span)?;
                }
            }
            ItemKind::Trait(t) => {
                self.text(&t.name, item.span)?;
                self.generics(&t.generics, depth + 1, subs, false)?;
                for bound in &t.bounds {
                    self.ty_at(bound, depth + 1, subs)?;
                }
                for member in &t.members {
                    self.generic_methods |=
                        matches!(&member.kind,ItemKind::Function(f) if !f.generics.is_empty());
                    self.item_at(member, depth + 1, subs)?;
                }
            }
            ItemKind::Const(c) => {
                self.text(&c.name, item.span)?;
                if let Some(ty) = &c.ty {
                    self.ty_at(ty, depth + 1, subs)?;
                }
                if let Some(value) = &c.value {
                    self.expr(value, depth + 1, subs)?;
                }
            }
            ItemKind::Extern { abi, function } => {
                self.text(abi, item.span)?;
                self.function(function, depth + 1, subs)?;
            }
            ItemKind::Meta(meta) => match meta {
                MetaItem::If {
                    condition,
                    then_items,
                    else_items,
                } => {
                    self.expr(condition, depth + 1, subs)?;
                    for item in then_items.iter().chain(else_items) {
                        self.item_at(item, depth + 1, subs)?;
                    }
                }
                MetaItem::For {
                    binding,
                    iterable,
                    items,
                } => {
                    self.text(binding, item.span)?;
                    self.expr(iterable, depth + 1, subs)?;
                    for item in items {
                        self.item_at(item, depth + 1, subs)?;
                    }
                }
                MetaItem::Emit(value) => self.expr(value, depth + 1, subs)?,
            },
        }
        Ok(())
    }
    fn pattern(&mut self, p: &Pattern, depth: usize) -> R<()> {
        self.node(p.span, depth)?;
        match &p.kind {
            PatternKind::Name(s) => self.text(s, p.span)?,
            PatternKind::Tuple(ps) => {
                for p in ps {
                    self.pattern(p, depth + 1)?;
                }
            }
            PatternKind::Constructor { path, arguments } => {
                self.type_member_patterns |= path.len() == 1;
                for s in path {
                    self.text(s, p.span)?;
                }
                for p in arguments {
                    self.pattern(p, depth + 1)?;
                }
            }
            PatternKind::RecordConstructor { path, fields } => {
                for name in path {
                    self.text(name, p.span)?;
                }
                for field in fields {
                    self.node(field.span, depth + 1)?;
                    self.text(&field.name, field.span)?;
                    self.pattern(&field.pattern, depth + 1)?;
                }
            }
            PatternKind::Reference { pattern, .. } => {
                self.reference_patterns = true;
                self.pattern(pattern, depth + 1)?;
            }
            PatternKind::ErrorArm { pattern, .. } => self.pattern(pattern, depth + 1)?,
            PatternKind::Literal(l) => self.literal(l, p.span)?,
            PatternKind::Wildcard => {}
        }
        Ok(())
    }
    fn literal(&mut self, l: &Literal, span: Span) -> R<()> {
        match l {
            Literal::Bool(_) => Ok(()),
            Literal::Integer(s) | Literal::Float(s) | Literal::String(s) | Literal::Char(s) => {
                self.text(s, span)
            }
        }
    }
    fn condition(
        &mut self,
        c: &Condition,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match c {
            Condition::Expression(e) => self.expr(e, depth, subs),
            Condition::Let { pattern, value } => {
                self.pattern(pattern, depth)?;
                self.expr(value, depth, subs)
            }
        }
    }
    fn block(
        &mut self,
        b: &Block,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(b.span, depth)?;
        for s in &b.statements {
            self.statement(s, depth + 1, subs)?;
        }
        Ok(())
    }
    pub(crate) fn statement_snapshot(&mut self, statement: &Stmt) -> R<()> {
        self.statement(statement, 0, None)
    }
    fn statement(
        &mut self,
        s: &Stmt,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(s.span, depth)?;
        match &s.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                self.pattern(pattern, depth + 1)?;
                if let Some(t) = ty {
                    self.ty_at(t, depth + 1, subs)?;
                }
                self.expr(value, depth + 1, subs)?;
            }
            StmtKind::Return(Some(e)) | StmtKind::Yield(e) | StmtKind::Expression(e) => {
                self.expr(e, depth + 1, subs)?
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, depth + 1, subs)?;
                self.expr(value, depth + 1, subs)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(condition, depth + 1, subs)?;
                self.block(then_body, depth + 1, subs)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, depth + 1, subs)?,
                    Some(ElseBranch::If(s)) => self.statement(s, depth + 1, subs)?,
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                self.condition(condition, depth + 1, subs)?;
                self.block(body, depth + 1, subs)?;
            }
            StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.pattern(pattern, depth + 1)?;
                self.expr(iterable, depth + 1, subs)?;
                self.block(body, depth + 1, subs)?;
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => {
                self.block(b, depth + 1, subs)?
            }
            StmtKind::With { name, value, body } => {
                self.text(name, s.span)?;
                self.expr(value, depth + 1, subs)?;
                self.block(body, depth + 1, subs)?;
            }
            StmtKind::Match { value, arms } => {
                self.expr(value, depth + 1, subs)?;
                for a in arms {
                    self.pattern(&a.pattern, depth + 1)?;
                    self.arrow(&a.body, depth + 1, subs)?;
                }
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Meta(_) => {
                return Err(unsupported(s.span, "generic expansion of meta statement"));
            }
        }
        Ok(())
    }
    fn arrow(
        &mut self,
        b: &ArrowBody,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match b {
            ArrowBody::Block(b) => self.block(b, depth, subs),
            ArrowBody::Expression(e) => self.expr(e, depth, subs),
        }
    }
    fn expr(&mut self, e: &Expr, depth: usize, subs: Option<&BTreeMap<String, TypeRef>>) -> R<()> {
        self.node(e.span, depth)?;
        if let ExprKind::Member {
            object,
            member: MemberName::Name(member),
        } = &e.kind
            && let ExprKind::Name(subject) = &object.kind
            && let Some(substitutions) = subs
            && substitutions
                .get(subject)
                .is_some_and(trait_bounds::constants::subject)
        {
            self.text(subject, e.span)?;
            self.text(member, e.span)?;
            self.text("\0constant:.", e.span)?;
            if let Some(value) = substitutions
                .get(&trait_bounds::constants::key(subject, member))
                .and_then(consts::expression)
            {
                return self.expr(value, depth, None);
            }
        }
        if let ExprKind::Name(name) = &e.kind
            && let Some(value) = subs
                .and_then(|substitutions| substitutions.get(name))
                .and_then(consts::expression)
        {
            return self.expr(value, depth, None);
        }
        if matches!(e.kind, ExprKind::ReceiverMember(_))
            && subs.is_some_and(|s| s.contains_key("Self"))
        {
            self.node(e.span, depth + 1)?;
            self.text("self", e.span)?;
        }
        match &e.kind {
            ExprKind::Ascribed { .. } => self.expr_ascribed(e, depth, subs),
            ExprKind::Literal(..) => self.expr_literal(e, depth, subs),
            ExprKind::Name(..) | ExprKind::ReceiverMember(..) => self.expr_name(e, depth, subs),
            ExprKind::Group(..)
            | ExprKind::Try(..)
            | ExprKind::MetaEval(..)
            | ExprKind::MetaSplice(..)
            | ExprKind::Unary { .. } => self.expr_unary(e, depth, subs),
            ExprKind::Tuple(..) | ExprKind::Array(..) => self.expr_sequence(e, depth, subs),
            ExprKind::Binary { .. } => self.expr_binary(e, depth, subs),
            ExprKind::Pipeline { .. } => self.expr_pipeline(e, depth, subs),
            ExprKind::Member { .. } => self.expr_member(e, depth, subs),
            ExprKind::TypeApply { .. } => self.expr_type_apply(e, depth, subs),
            ExprKind::Call { .. } => self.expr_call(e, depth, subs),
            ExprKind::Index { .. } => self.expr_index(e, depth, subs),
            ExprKind::Slice { .. } => self.expr_slice(e, depth, subs),
            ExprKind::Record { .. } => self.expr_record(e, depth, subs),
            ExprKind::Closure { .. } => self.expr_closure(e, depth, subs),
            ExprKind::If { .. } => self.expr_if(e, depth, subs),
            ExprKind::MetaBlock(..) => self.expr_meta_block(e, depth, subs),
            ExprKind::SyntaxIsland { .. } | ExprKind::Quote { .. } => {
                self.expr_syntax(e, depth, subs)
            }
        }
    }
    fn expr_ascribed(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        let ExprKind::Ascribed { ty, value } = &e.kind else {
            unreachable!("budget expression dispatch")
        };
        self.ty_at(ty, depth + 1, subs)?;
        self.expr(value, depth + 1, subs)
    }
    fn expr_literal(
        &mut self,
        e: &Expr,
        _depth: usize,
        _subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Literal(l) => {
                self.literal(l, e.span)?;
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_name(
        &mut self,
        e: &Expr,
        _depth: usize,
        _subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Name(s) | ExprKind::ReceiverMember(s) => {
                self.text(s, e.span)?;
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_unary(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Group(x)
            | ExprKind::Try(x)
            | ExprKind::MetaEval(x)
            | ExprKind::MetaSplice(x)
            | ExprKind::Unary { operand: x, .. } => {
                self.expr(x, depth + 1, subs)?;
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_sequence(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
                {
                    for x in xs {
                        self.expr(x, depth + 1, subs)?;
                    }
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_binary(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Binary { left, right, .. } => {
                {
                    self.expr(left, depth + 1, subs)?;
                    self.expr(right, depth + 1, subs)?;
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_pipeline(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Pipeline { input, target } => {
                {
                    self.expr(input, depth + 1, subs)?;
                    self.expr(target, depth + 1, subs)?;
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_member(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Member { object, member } => {
                {
                    self.expr(object, depth + 1, subs)?;
                    match member {
                        MemberName::Name(name) => self.text(name, e.span)?,
                        MemberName::Constrained { name, owner } => {
                            self.text(name, e.span)?;
                            self.ty_at(owner, depth + 1, subs)?;
                        }
                        MemberName::TupleIndex(_) => {}
                    }
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_type_apply(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::TypeApply { callee, arguments } => {
                {
                    self.expr(callee, depth + 1, subs)?;
                    for a in arguments {
                        self.arg(a, depth + 1, subs)?;
                    }
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_call(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Call { callee, arguments } => {
                {
                    self.expr(callee, depth + 1, subs)?;
                    for a in arguments {
                        self.expr(&a.value, depth + 1, subs)?;
                    }
                    self.note_static_task_create(e);
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_index(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Index { object, index } => {
                {
                    self.expr(object, depth + 1, subs)?;
                    self.expr(index, depth + 1, subs)?;
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_slice(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Slice { object, start, end } => {
                {
                    self.expr(object, depth + 1, subs)?;
                    for x in [start, end].into_iter().flatten() {
                        self.expr(x, depth + 1, subs)?;
                    }
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_record(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Record { ty, base, fields } => {
                {
                    for x in [ty, base].into_iter().flatten() {
                        self.expr(x, depth + 1, subs)?;
                    }
                    for f in fields {
                        self.text(&f.name, f.span)?;
                        if let Some(x) = &f.value {
                            self.expr(x, depth + 1, subs)?;
                        }
                    }
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_closure(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                {
                    for p in params {
                        self.node(p.span, depth + 1)?;
                        self.text(&p.name, p.span)?;
                        if let Some(t) = &p.ty {
                            self.ty_at(t, depth + 1, subs)?;
                        }
                    }
                    if let Some(t) = return_type {
                        self.ty_at(t, depth + 1, subs)?;
                    }
                    self.arrow(body, depth + 1, subs)?;
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_if(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                {
                    self.expr(condition, depth + 1, subs)?;
                    self.expr(then_value, depth + 1, subs)?;
                    self.expr(else_value, depth + 1, subs)?;
                };
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_meta_block(
        &mut self,
        e: &Expr,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::MetaBlock(b) => {
                self.block(b, depth + 1, subs)?;
            }
            _ => unreachable!("budget expression dispatch"),
        }
        Ok(())
    }
    fn expr_syntax(
        &mut self,
        e: &Expr,
        _depth: usize,
        _subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match &e.kind {
            ExprKind::SyntaxIsland { .. } | ExprKind::Quote { .. } => Err(unsupported(
                e.span,
                "generic expansion of unexecuted syntax island",
            )),
            _ => unreachable!("budget expression dispatch"),
        }
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[test]
    fn borrowed_visits_release_scratch_and_preserve_retained_bytes_at_exact_limit() {
        let peak = 128 + 1024 + 73;
        let limits = Limits {
            max_source_bytes: peak,
            ..Limits::default()
        };
        let mut budget = Budget::new(&limits);
        budget.lineage_bytes(128, Span::default()).unwrap();
        let available = budget.remaining_limits().max_source_bytes;
        for _ in 0..2 {
            budget
                .borrowed_work(Span::default(), |budget| {
                    budget.identity_work(73, Span::default())
                })
                .unwrap();
            assert_eq!(budget.remaining_limits().max_source_bytes, available);
        }
        let tight = Limits {
            max_source_bytes: peak - 1,
            ..Limits::default()
        };
        let mut failed = Budget::new(&tight);
        failed.lineage_bytes(128, Span::default()).unwrap();
        assert!(
            failed
                .borrowed_work(Span::default(), |budget| {
                    budget.identity_work(73, Span::default())
                })
                .is_err()
        );
        assert_eq!(failed.remaining_limits().max_source_bytes, peak - 1 - 128);
        // A previous failed request never changes a fresh default request.
        let mut fresh = Budget::new(&limits);
        fresh.lineage_bytes(128, Span::default()).unwrap();
        fresh
            .borrowed_work(Span::default(), |budget| {
                budget.identity_work(73, Span::default())
            })
            .unwrap();
    }

    #[test]
    fn retained_copy_keeps_its_bytes_until_the_actual_vector_is_dropped() {
        let limits = Limits {
            max_source_bytes: 128 + 1024 + 73,
            ..Limits::default()
        };
        let mut budget = Budget::new(&limits);
        budget.lineage_bytes(128, Span::default()).unwrap();
        let value = budget
            .retained_copy(Span::default(), |budget| {
                budget.identity_work(73, Span::default())?;
                Ok(vec![0u8; 73])
            })
            .unwrap();
        assert_eq!(
            budget.remaining_limits().max_source_bytes,
            limits.max_source_bytes - 128 - 73
        );
        budget.drop_lineage_vec(value, Span::default()).unwrap();
        assert_eq!(
            budget.remaining_limits().max_source_bytes,
            limits.max_source_bytes - 128
        );
    }

    #[test]
    fn vector_growth_prepays_both_live_allocations_and_preserves_input_on_failure() {
        let run = |bytes| {
            let limits = Limits {
                max_source_bytes: bytes,
                ..Limits::default()
            };
            let mut budget = Budget::new(&limits);
            let mut rows = Vec::new();
            for value in 0..4u8 {
                budget
                    .lineage_push(&mut rows, value, Span::default())
                    .unwrap();
            }
            let grown = budget.lineage_push(&mut rows, 4, Span::default());
            (
                grown.is_ok(),
                rows,
                budget.remaining_limits().max_source_bytes,
            )
        };
        let (ok, rows, remaining) = run(12);
        assert!(ok);
        assert_eq!(rows, [0, 1, 2, 3, 4]);
        assert_eq!(remaining, 4);
        let (ok, rows, remaining) = run(11);
        assert!(!ok);
        assert_eq!(rows, [0, 1, 2, 3]);
        assert_eq!(remaining, 7);
        assert!(run(12).0);
    }
    #[test]
    fn child_first_allocation_is_adopted_without_a_prior_lineage_debit() {
        let limits = Limits {
            max_source_bytes: 1000,
            ..Limits::default()
        };
        let mut budget = Budget::new(&limits);
        let remaining = Limits {
            max_source_bytes: 927,
            ..limits.clone()
        };
        budget.adopt_remaining(&remaining);
        assert_eq!(budget.remaining_limits().max_source_bytes, 927);
    }

    #[test]
    fn nested_scopes_share_the_live_peak_and_keep_the_nested_owner() {
        let peak = 128 + 2 * 1024 + 73;
        let limits = Limits {
            max_source_bytes: peak,
            ..Limits::default()
        };
        let mut budget = Budget::new(&limits);
        budget.lineage_bytes(128, Span::default()).unwrap();
        let mut held = None;
        budget
            .borrowed_work(Span::default(), |budget| {
                held = Some(budget.retained_copy(Span::default(), |budget| {
                    budget.identity_work(73, Span::default())?;
                    Ok(vec![0u8; 73])
                })?);
                Ok(())
            })
            .unwrap();
        assert_eq!(budget.remaining_limits().max_source_bytes, peak - 128 - 73);
        budget
            .drop_lineage_vec(held.unwrap(), Span::default())
            .unwrap();
        assert_eq!(budget.remaining_limits().max_source_bytes, peak - 128);
        let tight = Limits {
            max_source_bytes: peak - 1,
            ..limits
        };
        let mut failed = Budget::new(&tight);
        failed.lineage_bytes(128, Span::default()).unwrap();
        assert!(
            failed
                .borrowed_work(Span::default(), |budget| {
                    budget
                        .retained_copy(Span::default(), |budget| {
                            budget.identity_work(73, Span::default())?;
                            Ok(vec![0u8; 73])
                        })
                        .map(drop)
                })
                .is_err()
        );
    }
}
