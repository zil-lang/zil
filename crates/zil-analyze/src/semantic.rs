//! The original lexical inventory is untrusted. Validate its exact links before
//! interpreting any class expression; this pass grants no memory permission.
use crate::{bad, budget::Budget};
use std::collections::{BTreeMap, BTreeSet};
use zil_base::CompileResult;
use zil_ir::*;
mod closed;
pub(super) mod frozen;
pub(super) mod geometry;
mod original;
mod original_index;
mod site_index;
mod value;

const WORK: &str = "original semantic program inventory";
fn fail(message: &str) -> Vec<zil_base::Diagnostic> {
    bad(format!("semantic program: {message}"))
}

#[derive(Clone, Copy)]
enum DynDefinitionLocation {
    Actual(usize),
    Source { module: usize, declaration: usize },
}

// Optional profiling coordinates only. These numeric IDs locate the active
// immutable world/site; they are never read by a semantic decision.
#[derive(Clone, Copy, Default)]
struct ProfilePoint {
    stage: &'static str,
    step: &'static str,
    instance: Option<SemanticInstanceId>,
    definition: Option<SemanticDefinitionId>,
    function: Option<FunctionId>,
    site_instance: Option<SemanticInstanceId>,
    recipe: Option<SemanticSiteId>,
    operation: Option<(BlockId, usize, SemanticSiteEdge)>,
}

struct Inventory<'a> {
    module: &'a TypedModule,
    program: &'a SemanticModule,
    limits: &'a zil_base::Limits,
    source: Option<&'a CoreProgram>,
    frozen_wrapper: Option<SemanticDefinitionId>,
    definitions: BTreeMap<SemanticDefinitionId, &'a SemanticDefinition>,
    // Immutable validated source structure; Arc preserves covariance of the
    // borrowed original nodes and grants no original or actual value fact.
    original_indexes: BTreeMap<SemanticDefinitionId, std::sync::Arc<original_index::Index<'a>>>,
    instances: BTreeMap<SemanticInstanceId, &'a SemanticInstance>,
    functions: BTreeMap<FunctionId, &'a TypedFunction>,
    // Pure original declarations, indexed only for missing-Names shadow
    // rejection. A name never selects a provider, value or builtin permission.
    top_level_names: BTreeMap<zil_base::ModuleId, Vec<(&'a str, bool)>>,
    // Successful immutable source-directory queries only. A frozen source
    // view is part of each key; none of these rows carries N/kappa or access.
    dyn_definitions: std::cell::RefCell<BTreeMap<(usize, LifeDeclarationId), DynDefinitionLocation>>,
    declaration_scopes:
        std::cell::RefCell<BTreeMap<(usize, usize, LifeDeclarationId), SemanticDefinitionId>>,
    drop_methods: std::cell::RefCell<BTreeMap<SemanticDefinitionId, Option<SemanticDefinitionId>>>,
    ordinary_drop_absence: std::cell::RefCell<BTreeSet<SemanticDefinitionId>>,
    // Successful immutable sealed-resource directory checks only, isolated
    // by the exact borrowed module and the current metadata limit profile.
    sealed_byte_buffers: std::cell::RefCell<BTreeSet<(usize, usize, usize)>>,
    geometry: geometry::Inputs,
    call_anchors: std::cell::RefCell<BTreeMap<SemanticDefinitionId, BTreeMap<zil_base::NodeId, SemanticSiteId>>>,
    emitted_sites: std::cell::RefCell<BTreeMap<usize, BTreeMap<(SemanticInstanceId, SemanticSiteId), usize>>>,
    function_worlds: std::cell::RefCell<BTreeMap<usize, BTreeMap<FunctionId, Vec<usize>>>>,
    actual_site_indexes: std::cell::RefCell<BTreeMap<usize, site_index::Index>>,
    life_declarations: BTreeMap<LifeDeclarationId, Vec<usize>>,
    life_instances: BTreeMap<LifeDeclarationId, Vec<usize>>,
    profile_point: std::cell::Cell<ProfilePoint>,
}
impl<'a> Inventory<'a> {
    fn new(
        module: &'a TypedModule,
        limits: &'a zil_base::Limits,
        program: &'a SemanticModule,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        // The generated, bounded visitor includes every type, class, constant,
        // identity, recipe and actual occurrence. No module hash runs first.
        crate::dyn_metadata::recipe(program, b)?;
        let mut inventory = Self {
            module,
            program,
            limits,
            source: None,
            frozen_wrapper: None,
            definitions: BTreeMap::new(),
            original_indexes: BTreeMap::new(),
            instances: BTreeMap::new(),
            functions: BTreeMap::new(),
            top_level_names: BTreeMap::new(),
            dyn_definitions: std::cell::RefCell::new(BTreeMap::new()),
            declaration_scopes: std::cell::RefCell::new(BTreeMap::new()),
            drop_methods: std::cell::RefCell::new(BTreeMap::new()),
            ordinary_drop_absence: std::cell::RefCell::new(BTreeSet::new()),
            sealed_byte_buffers: std::cell::RefCell::new(BTreeSet::new()),
            geometry: geometry::Inputs::new(module, program, b)?,
            call_anchors: std::cell::RefCell::new(BTreeMap::new()),
            emitted_sites: std::cell::RefCell::new(BTreeMap::new()),
            function_worlds: std::cell::RefCell::new(BTreeMap::new()),
            actual_site_indexes: std::cell::RefCell::new(BTreeMap::new()),
            life_declarations: BTreeMap::new(),
            life_instances: BTreeMap::new(),
            profile_point: std::cell::Cell::new(ProfilePoint::default()),
        };
        let mut identities: Vec<(&zil_ir::LifeDeclarationIdentity, usize)> = Vec::new();
        for definition in &program.definitions {
            b.lookup(inventory.definitions.len(), WORK)?;
            let identity = &definition.identity;
            b.spend(
                identity
                    .module_path
                    .len()
                    .saturating_add(identity.declaration_path.len()),
                WORK,
            )?;
            let width = identity
                .module_path
                .iter()
                .chain(&identity.declaration_path)
                .fold(
                    identity
                        .package
                        .len()
                        .saturating_add(identity.generation_path.len())
                        .saturating_add(identity.module_path.len())
                        .saturating_add(identity.declaration_path.len())
                        .saturating_add(1),
                    |size, part| size.saturating_add(part.len()),
                );
            // Each actual full Ord comparison is prepaid. Keeping sorted
            // borrowed identities avoids a black-box tree comparison charged
            // as a scan of every prior complete string key.
            let (mut low, mut high) = (0usize, identities.len());
            while low < high {
                let middle = low + (high - low) / 2;
                let (candidate, candidate_width) = identities[middle];
                b.spend(width.saturating_add(candidate_width).saturating_add(1), WORK)?;
                match identity.cmp(candidate) {
                    std::cmp::Ordering::Less => high = middle,
                    std::cmp::Ordering::Greater => low = middle + 1,
                    std::cmp::Ordering::Equal => return Err(fail("original definition identity is ambiguous")),
                }
            }
            if inventory.definitions.insert(definition.id, definition).is_some() {
                return Err(fail("original definition identity is ambiguous"));
            }
            let relocation = if identities.len() == identities.capacity() { identities.len() } else { 0 };
            b.spend(identities.len().saturating_sub(low).saturating_add(relocation).saturating_mul(2).saturating_add(1), WORK)?;
            identities.try_reserve(1).map_err(|_| fail("original identity directory allocation failed"))?;
            identities.insert(low, (identity, width));
            inventory.index_top_level_name(definition, b)?;
        }
        for instance in &program.instances {
            b.lookup(inventory.instances.len(), WORK)?;
            if inventory.instances.insert(instance.id, instance).is_some() {
                return Err(fail("lexical instance identity is ambiguous"));
            }
        }
        for function in &module.functions {
            b.lookup(inventory.functions.len(), WORK)?;
            if inventory.functions.insert(function.id, function).is_some() {
                return Err(fail("actual function identity is ambiguous"));
            }
        }
        for (position, declaration) in module.life_declarations.iter().enumerate() {
            site_index::append(&mut inventory.life_declarations, declaration.id, position, b)?;
        }
        for (position, instance) in module.life_instances.iter().enumerate() {
            site_index::append(&mut inventory.life_instances, instance.declaration, position, b)?;
        }
        for definition in inventory.definitions.values() {
            if matches!(definition.body, SemanticDefinitionBody::FunctionWithBody { .. }) {
                let index = original_index::index(definition, b)?;
                b.lookup(inventory.original_indexes.len(), WORK)?;
                inventory.original_indexes.insert(definition.id, std::sync::Arc::new(index));
            }
        }
        Ok(inventory)
    }
    fn profile_stage(&self, stage: &'static str, b: &Budget) {
        if b.cost_profiling() {
            self.profile_point.set(ProfilePoint { stage, ..ProfilePoint::default() });
        }
    }
    fn profile_step(&self, step: &'static str, b: &Budget) {
        if b.cost_profiling() {
            let mut point = self.profile_point.get();
            point.step = step;
            point.site_instance = None;
            point.recipe = None;
            point.operation = None;
            self.profile_point.set(point);
        }
    }
    fn profile_instance(&self, instance: &SemanticInstance, b: &Budget) {
        if b.cost_profiling() {
            let old = self.profile_point.get();
            self.profile_point.set(ProfilePoint {
                stage: old.stage,
                step: old.step,
                instance: Some(instance.id),
                definition: Some(instance.definition),
                function: match instance.target {
                    SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) => Some(id),
                    _ => None,
                },
                ..ProfilePoint::default()
            });
        }
    }
    fn profile_site(&self, site: &SemanticSite, b: &Budget) {
        if b.cost_profiling() {
            let mut point = self.profile_point.get();
            point.site_instance = Some(site.instance);
            point.recipe = Some(site.recipe);
            point.operation = match site.at {
                SemanticSiteAddress::Typed { block, instruction, edge } => Some((block, instruction as usize, edge)),
                _ => None,
            };
            self.profile_point.set(point);
        }
    }
    fn profile_recipe(&self, recipe: SemanticSiteId, b: &Budget) {
        if b.cost_profiling() {
            let mut point = self.profile_point.get();
            point.site_instance = point.instance;
            point.recipe = Some(recipe);
            point.operation = None;
            self.profile_point.set(point);
        }
    }
    fn profile_operation(&self, block: BlockId, instruction: usize, b: &Budget) {
        if b.cost_profiling() {
            let mut point = self.profile_point.get();
            point.site_instance = None;
            point.recipe = None;
            point.operation = Some((block, instruction, SemanticSiteEdge::Before));
            self.profile_point.set(point);
        }
    }
    fn profile_errors(&self, mut errors: Vec<zil_base::Diagnostic>, b: &Budget) -> Vec<zil_base::Diagnostic> {
        if b.cost_profiling() {
            let point = self.profile_point.get();
            for error in errors.iter_mut().take(4) {
                error.notes.push(format!(
                    "analysis original context module {} phase {} step {} world {:?} definition {:?} function {:?} examined-site-world {:?} recipe {:?} operation {:?} metadata profile ({},{})",
                    self.module.module_id.0, point.stage, point.step, point.instance,
                    point.definition, point.function, point.site_instance, point.recipe,
                    point.operation, b.partial_depth, b.helper_nodes,
                ));
                error.notes.extend(b.geometry_cost_notes());
            }
        }
        errors
    }
    fn index_top_level_name(&mut self, definition: &'a SemanticDefinition, b: &mut Budget) -> CompileResult<()> {
        if definition.parent.is_some() { return Ok(()); }
        let Some(name) = definition.identity.declaration_path.last() else { return Ok(()); };
        b.lookup(self.top_level_names.len(), WORK)?;
        let rows = self.top_level_names.entry(definition.id.module).or_default();
        let (mut low, mut high) = (0usize, rows.len());
        while low < high {
            let middle = low + (high - low) / 2;
            b.spend(name.len().saturating_add(rows[middle].0.len()).saturating_add(1), WORK)?;
            match name.as_str().cmp(rows[middle].0) {
                std::cmp::Ordering::Less => high = middle,
                std::cmp::Ordering::Greater => low = middle + 1,
                std::cmp::Ordering::Equal => {
                    rows[middle].1 |= definition.kind == SemanticDefinitionKind::Function;
                    return Ok(());
                }
            }
        }
        let relocation = if rows.len() == rows.capacity() { rows.len() } else { 0 };
        b.spend(rows.len().saturating_sub(low).saturating_add(relocation).saturating_mul(2).saturating_add(1), WORK)?;
        rows.try_reserve(1).map_err(|_| fail("original name directory allocation failed"))?;
        rows.insert(low, (name, definition.kind == SemanticDefinitionKind::Function));
        Ok(())
    }
    fn hidden_top_level(&self, module: zil_base::ModuleId, name: &str, numeric: bool,
        b: &mut Budget) -> CompileResult<bool> {
        b.lookup(self.top_level_names.len(), WORK)?;
        let Some(rows) = self.top_level_names.get(&module) else { return Ok(false); };
        let (mut low, mut high) = (0usize, rows.len());
        while low < high {
            let middle = low + (high - low) / 2;
            b.spend(name.len().saturating_add(rows[middle].0.len()).saturating_add(1), WORK)?;
            match name.cmp(rows[middle].0) {
                std::cmp::Ordering::Less => high = middle,
                std::cmp::Ordering::Greater => low = middle + 1,
                std::cmp::Ordering::Equal => return Ok(numeric || rows[middle].1),
            }
        }
        Ok(false)
    }
    fn original_index(
        &self,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<std::sync::Arc<original_index::Index<'a>>> {
        b.lookup(self.original_indexes.len(), WORK)?;
        b.spend(1, WORK)?;
        self.original_indexes.get(&owner).cloned()
            .ok_or_else(|| fail("original function has no complete structural directory"))
    }
    fn source_key(&self) -> usize {
        self.source
            .map_or(0, |source| source as *const CoreProgram as usize)
    }
    #[track_caller]
    fn definition(
        &self,
        id: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticDefinition> {
        b.lookup(self.definitions.len(), WORK)?;
        self.definitions
            .get(&id)
            .copied()
            .ok_or_else(|| fail("original lexical definition is absent"))
    }
    #[track_caller]
    fn instance(
        &self,
        id: SemanticInstanceId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticInstance> {
        b.lookup(self.instances.len(), WORK)?;
        self.instances
            .get(&id)
            .copied()
            .ok_or_else(|| fail("actual lexical instance is absent"))
    }
    #[track_caller]
    fn function(
        &self,
        target: &SemanticFunctionTarget,
        b: &mut Budget,
    ) -> CompileResult<&'a TypedFunction> {
        let SemanticFunctionTarget::Typed(id) = target else {
            return Err(fail("executable program retains a Core function target"));
        };
        b.lookup(self.functions.len(), WORK)?;
        self.functions
            .get(id)
            .copied()
            .ok_or_else(|| fail("exact actual function target is absent"))
    }
    fn call_anchor(
        &self,
        owner: SemanticDefinitionId,
        node: zil_base::NodeId,
        b: &mut Budget,
    ) -> CompileResult<SemanticSiteId> {
        b.lookup(self.call_anchors.borrow().len(), WORK)?;
        if !self.call_anchors.borrow().contains_key(&owner) {
            let original = self.definition(owner, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                sites, site_nodes, ..
            } = &original.body
            else {
                return Err(fail("original call anchor has no complete function body"));
            };
            let mut recipes = BTreeMap::new();
            for site in sites {
                b.lookup(recipes.len(), WORK)?;
                if recipes
                    .insert(
                        site.id,
                        matches!(
                            site.kind,
                            SemanticSiteRecipeKind::OriginalCall
                                | SemanticSiteRecipeKind::Call { .. }
                                | SemanticSiteRecipeKind::CallableCall { .. }
                        ),
                    )
                    .is_some()
                {
                    return Err(fail("original call anchor has repeated recipe identities"));
                }
            }
            let mut anchors = BTreeMap::new();
            for link in site_nodes {
                b.lookup(recipes.len(), WORK)?;
                if recipes.get(&link.site) != Some(&true) {
                    continue;
                }
                if let SemanticOriginalNode::Expression(node) = link.node {
                    b.lookup(anchors.len(), WORK)?;
                    if anchors.insert(node, link.site).is_some() {
                        return Err(fail("original call result has ambiguous role anchors"));
                    }
                }
            }
            b.lookup(self.call_anchors.borrow().len(), WORK)?;
            self.call_anchors.borrow_mut().insert(owner, anchors);
        }
        let anchors = self.call_anchors.borrow();
        let anchors = anchors
            .get(&owner)
            .ok_or_else(|| fail("original call anchor directory is absent"))?;
        b.lookup(anchors.len(), WORK)?;
        anchors
            .get(&node)
            .copied()
            .ok_or_else(|| fail("original call result has no emitted role anchor"))
    }
    fn owns_program(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if std::ptr::eq(self.program, program) {
            return Ok(true);
        }
        if let Some(source) = self.source {
            for module in &source.modules {
                b.spend(1, WORK)?;
                if module
                    .core
                    .semantic
                    .as_ref()
                    .is_some_and(|input| std::ptr::eq(input, program))
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn ensure_actual_sites(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<bool> {
        // A cache owner must be the held actual module or a module reached
        // through the Inventory's held immutable CoreProgram. Temporaries scan.
        if !self.owns_program(program, b)? {
            return Ok(false);
        }
        let owner = program as *const SemanticModule as usize;
        b.lookup(self.actual_site_indexes.borrow().len(), WORK)?;
        if !self.actual_site_indexes.borrow().contains_key(&owner) {
            let index = site_index::Index::new(program, b)?;
            b.lookup(self.actual_site_indexes.borrow().len(), WORK)?;
            self.actual_site_indexes.borrow_mut().insert(owner, index);
        }
        Ok(true)
    }
    fn sites_at<'p>(
        &self,
        program: &'p SemanticModule,
        instance: SemanticInstanceId,
        block: BlockId,
        instruction: usize,
        edge: SemanticSiteEdge,
        b: &mut Budget,
    ) -> CompileResult<Vec<&'p SemanticSite>> {
        let Ok(instruction) = u32::try_from(instruction) else {
            return Ok(Vec::new());
        };
        let matches = |site: &&SemanticSite| {
            site.instance == instance
                && matches!(site.at,
            SemanticSiteAddress::Typed { block: actual, instruction: ordinal, edge: boundary }
                if actual == block && ordinal == instruction && boundary == edge)
        };
        if !self.ensure_actual_sites(program, b)? {
            let mut selected = Vec::new();
            for site in &program.sites {
                b.spend(1, WORK)?;
                if matches(&site) {
                    Self::push_site(&mut selected, site, b)?;
                }
            }
            return Ok(selected);
        }
        let indexes = self.actual_site_indexes.borrow();
        let index = indexes
            .get(&(program as *const SemanticModule as usize))
            .ok_or_else(|| fail("actual site owner is absent"))?;
        let mut selected = Vec::new();
        for position in index.address((instance, block, instruction, site_index::edge(edge)), b)? {
            let site = program
                .sites
                .get(*position)
                .filter(matches)
                .ok_or_else(|| fail("actual site address changes its immutable coordinate"))?;
            Self::push_site(&mut selected, site, b)?;
        }
        Ok(selected)
    }
    fn sites_for_value<'p>(
        &self,
        program: &'p SemanticModule,
        instance: SemanticInstanceId,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<Vec<&'p SemanticSite>> {
        if !self.ensure_actual_sites(program, b)? {
            let mut selected = Vec::new();
            for site in &program.sites {
                b.spend(1, WORK)?;
                if site.instance == instance
                    && site_index::operand(site) == Some(site_index::OperandKey::Value(value))
                {
                    Self::push_site(&mut selected, site, b)?;
                }
            }
            return Ok(selected);
        }
        let indexes = self.actual_site_indexes.borrow();
        let index = indexes
            .get(&(program as *const SemanticModule as usize))
            .ok_or_else(|| fail("actual operand owner is absent"))?;
        let mut selected = Vec::new();
        for position in index.operand(instance, site_index::OperandKey::Value(value), b)? {
            let site = program
                .sites
                .get(*position)
                .ok_or_else(|| fail("actual operand leaves its immutable event table"))?;
            if site.instance != instance
                || site_index::operand(site) != Some(site_index::OperandKey::Value(value))
            {
                return Err(fail("actual operand changes its original world or kind"));
            }
            Self::push_site(&mut selected, site, b)?;
        }
        Ok(selected)
    }
    fn push_site<'p>(
        rows: &mut Vec<&'p SemanticSite>,
        site: &'p SemanticSite,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(
            1 + if rows.len() == rows.capacity() {
                rows.len()
            } else {
                0
            },
            WORK,
        )?;
        rows.try_reserve(1)
            .map_err(|_| fail("actual event candidate allocation failed"))?;
        rows.push(site);
        Ok(())
    }
    fn nominal_declarations(
        &self,
        declaration: LifeDeclarationId,
        b: &mut Budget,
    ) -> CompileResult<Vec<&'a LifeNominalDeclaration>> {
        b.lookup(self.life_declarations.len(), WORK)?;
        let mut rows = Vec::new();
        for position in self
            .life_declarations
            .get(&declaration)
            .map_or(&[][..], Vec::as_slice)
        {
            b.spend(
                1 + if rows.len() == rows.capacity() {
                    rows.len()
                } else {
                    0
                },
                WORK,
            )?;
            rows.try_reserve(1)
                .map_err(|_| fail("nominal declaration candidate allocation failed"))?;
            rows.push(
                self.module
                    .life_declarations
                    .get(*position)
                    .filter(|row| row.id == declaration)
                    .ok_or_else(|| fail("nominal declaration coordinate differs"))?,
            );
        }
        Ok(rows)
    }
    fn nominal_instances(
        &self,
        declaration: LifeDeclarationId,
        b: &mut Budget,
    ) -> CompileResult<Vec<(usize, &'a LifeNominalInstance)>> {
        b.lookup(self.life_instances.len(), WORK)?;
        let mut rows = Vec::new();
        for position in self
            .life_instances
            .get(&declaration)
            .map_or(&[][..], Vec::as_slice)
        {
            b.spend(
                1 + if rows.len() == rows.capacity() {
                    rows.len()
                } else {
                    0
                },
                WORK,
            )?;
            rows.try_reserve(1)
                .map_err(|_| fail("nominal instance candidate allocation failed"))?;
            rows.push((
                *position,
                self.module
                    .life_instances
                    .get(*position)
                    .filter(|row| row.declaration == declaration)
                    .ok_or_else(|| fail("nominal instance coordinate differs"))?,
            ));
        }
        Ok(rows)
    }
    fn emitted_site<'p>(
        &self,
        program: &'p SemanticModule,
        instance: SemanticInstanceId,
        recipe: SemanticSiteId,
        b: &mut Budget,
    ) -> CompileResult<&'p SemanticSite> {
        if !self.owns_program(program, b)? {
            let mut selected = None;
            for site in &program.sites {
                b.spend(1, WORK)?;
                if site.instance == instance
                    && site.recipe == recipe
                    && selected.replace(site).is_some()
                {
                    return Err(fail("original result role has multiple executable calls"));
                }
            }
            return selected.ok_or_else(|| fail("original result role has no actual call"));
        }
        let owner = program as *const SemanticModule as usize;
        b.lookup(self.emitted_sites.borrow().len(), WORK)?;
        if !self.emitted_sites.borrow().contains_key(&owner) {
            let mut sites = BTreeMap::new();
            for (position, site) in program.sites.iter().enumerate() {
                b.lookup(sites.len(), WORK)?;
                if sites
                    .insert((site.instance, site.recipe), position)
                    .is_some()
                {
                    return Err(fail("original result role has multiple executable calls"));
                }
            }
            b.lookup(self.emitted_sites.borrow().len(), WORK)?;
            self.emitted_sites.borrow_mut().insert(owner, sites);
        }
        let sites = self.emitted_sites.borrow();
        let sites = sites
            .get(&owner)
            .ok_or_else(|| fail("actual event coordinate directory is absent"))?;
        b.lookup(sites.len(), WORK)?;
        let position = *sites
            .get(&(instance, recipe))
            .ok_or_else(|| fail("original result role has no actual call"))?;
        program
            .sites
            .get(position)
            .filter(|site| site.instance == instance && site.recipe == recipe)
            .ok_or_else(|| fail("actual event coordinate differs from its complete program"))
    }
    fn worlds<'p>(
        &self,
        program: &'p SemanticModule,
        function: FunctionId,
        b: &mut Budget,
    ) -> CompileResult<Vec<&'p SemanticInstance>> {
        if !self.owns_program(program, b)? {
            let mut worlds = Vec::new();
            for instance in &program.instances {
                b.spend(1, WORK)?;
                if matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == function)
                {
                    b.spend(
                        1 + if worlds.len() == worlds.capacity() {
                            worlds.len()
                        } else {
                            0
                        },
                        WORK,
                    )?;
                    worlds.push(instance);
                }
            }
            return Ok(worlds);
        }
        let owner = program as *const SemanticModule as usize;
        b.lookup(self.function_worlds.borrow().len(), WORK)?;
        if !self.function_worlds.borrow().contains_key(&owner) {
            let mut worlds = BTreeMap::<FunctionId, Vec<usize>>::new();
            for (position, instance) in program.instances.iter().enumerate() {
                b.spend(1, WORK)?;
                if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
                    instance.target
                {
                    b.lookup(worlds.len(), WORK)?;
                    let entries = worlds.entry(function).or_default();
                    b.spend(
                        1 + if entries.len() == entries.capacity() {
                            entries.len()
                        } else {
                            0
                        },
                        WORK,
                    )?;
                    entries.push(position);
                }
            }
            b.lookup(self.function_worlds.borrow().len(), WORK)?;
            self.function_worlds.borrow_mut().insert(owner, worlds);
        }
        let worlds = self.function_worlds.borrow();
        let worlds = worlds
            .get(&owner)
            .ok_or_else(|| fail("actual function-world directory is absent"))?;
        b.lookup(worlds.len(), WORK)?;
        let positions = worlds.get(&function).map_or(&[][..], Vec::as_slice);
        b.spend(positions.len(), WORK)?;
        positions.iter().map(|position| program.instances.get(*position).filter(|instance| matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == function)).ok_or_else(|| fail("actual function world changes its exact program coordinate"))).collect()
    }
    fn resolved(value: &SemanticTypeValue, b: &mut Budget) -> CompileResult<()> {
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(fail("executable program retains a Core type operand"));
        };
        crate::dyn_metadata::ty(ty, b)?;
        b.spend(classes.len().saturating_add(1), WORK)?;
        if classes.len() != life.binder_count as usize {
            return Err(fail("resolved type loses an intrinsic class occurrence"));
        }
        for class in classes {
            let operand = match class {
                SemanticClassExpr::OuterOccurrence { operand, .. }
                | SemanticClassExpr::IntrinsicClass { operand, .. } => Some(operand),
                _ => None,
            };
            if matches!(operand, Some(SemanticOperand::CoreExpression { .. })) {
                return Err(fail("resolved class retains a Core expression selector"));
            }
        }
        // Equality of the complete intrinsic shape and every original recipe is
        // a separate obligation. Count equality alone never supplies that proof.
        Ok(())
    }
    fn actuals(actuals: &[SemanticActual], b: &mut Budget) -> CompileResult<()> {
        b.spend(actuals.len(), WORK)?;
        for actual in actuals {
            match actual {
                SemanticActual::Type(value) | SemanticActual::Const { ty: value, .. } => {
                    Self::resolved(value, b)?
                }
                SemanticActual::CoreRecipe { .. } | SemanticActual::CoreInferred { .. } => {
                    return Err(fail("executable program retains an unresolved Core actual"));
                }
                SemanticActual::Life(
                    SemanticClassExpr::OuterOccurrence {
                        operand: SemanticOperand::CoreExpression { .. },
                        ..
                    }
                    | SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::CoreExpression { .. },
                        ..
                    },
                ) => {
                    return Err(fail("actual lifetime retains a Core expression selector"));
                }
                SemanticActual::Life(_) => {}
            }
        }
        Ok(())
    }
    fn parents(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<()> {
        for definition in &program.definitions {
            let mut implicit_paths = BTreeSet::new();
            for (ordinal, row) in definition.implicit_lifetimes.iter().enumerate() {
                b.spend(
                    implicit_paths
                        .len()
                        .saturating_mul(row.occurrence_path.len().saturating_add(1))
                        .saturating_add(1),
                    WORK,
                )?;
                if row.ordinal as usize != ordinal || !implicit_paths.insert(&row.occurrence_path) {
                    return Err(fail(
                        "original implicit occurrence directory is not exact and dense",
                    ));
                }
            }
            let mut active = BTreeSet::new();
            let mut current = Some(definition.id);
            while let Some(id) = current {
                b.spend(active.len().saturating_add(1), WORK)?;
                if !active.insert(id) {
                    return Err(fail("original lexical parent graph is cyclic"));
                }
                current = self.definition(id, b)?.parent;
            }
        }
        for instance in &program.instances {
            let definition = self.definition(instance.definition, b)?;
            Self::actuals(&instance.actuals, b)?;
            if let Some(value) = &instance.self_type {
                Self::resolved(value, b)?;
            }
            if let Some(environment) = &instance.environment {
                Self::actuals(&environment.actuals, b)?;
                if !matches!(environment.closure, SemanticClosureTarget::Typed(_)) {
                    return Err(fail("environment retains a Core closure target"));
                }
            }
            if instance.actuals.len() != definition.parameters.len() {
                return Err(fail("instance omits an original mixed formal"));
            }
            match (instance.parent, definition.parent) {
                (None, None) => {}
                (Some(parent), Some(expected))
                    if self.instance(parent, b)?.definition == expected => {}
                _ => return Err(fail("instance changes its original lexical parent")),
            }
            let mut active = BTreeSet::new();
            let mut current = Some(instance.id);
            while let Some(id) = current {
                b.spend(active.len().saturating_add(1), WORK)?;
                if !active.insert(id) {
                    return Err(fail("actual lexical parent graph is cyclic"));
                }
                current = self.instance(id, b)?.parent;
            }
            match (&instance.target, &definition.body) {
                (
                    SemanticInstanceTarget::Function(target),
                    SemanticDefinitionBody::FunctionWithBody { .. },
                ) => {
                    self.function(target, b)?;
                }
                (
                    SemanticInstanceTarget::Scope,
                    SemanticDefinitionBody::Nominal { .. } | SemanticDefinitionBody::Scope(_),
                ) => {
                    if !instance.header_embedding.is_empty() {
                        return Err(fail("nonfunction scope invents runtime header binders"));
                    }
                }
                _ => return Err(fail("instance changes its original definition kind")),
            }
            if let SemanticLifetimeInstantiation::Partition { rigid } =
                instance.lifetime_instantiation
            {
                let original = self.instance(rigid, b)?;
                if original.definition != instance.definition
                    || !matches!(
                        original.lifetime_instantiation,
                        SemanticLifetimeInstantiation::Rigid
                    )
                {
                    return Err(fail("partition has no exact original rigid instance"));
                }
            }
        }
        // Some is a complete original program claim. An empty side table
        // cannot bypass declaration obligations for a real executable entry.
        for function in self.functions.values() {
            b.spend(program.instances.len().saturating_add(1), WORK)?;
            let count = program.instances.iter().filter(|instance| matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == function.id)).count();
            if count == 0 {
                return Err(fail("actual function has no complete original instance"));
            }
        }
        Ok(())
    }
    fn sites(&self, program: &SemanticModule, b: &mut Budget) -> CompileResult<()> {
        let mut emitted = BTreeSet::new();
        let mut consumed = BTreeSet::new();
        for site in &program.sites {
            let instance = self.instance(site.instance, b)?;
            let definition = self.definition(instance.definition, b)?;
            let (
                SemanticInstanceTarget::Function(target),
                SemanticDefinitionBody::FunctionWithBody { .. },
            ) = (&instance.target, &definition.body)
            else {
                return Err(fail("emission site does not belong to an actual function"));
            };
            let function = self.function(target, b)?;
            b.lookup(consumed.len(), WORK)?;
            b.lookup(emitted.len(), WORK)?;
            if !consumed.insert((site.instance, site.recipe)) {
                return Err(fail(
                    "one original semantic event is claimed by multiple actual emissions",
                ));
            }
            let directory = self.original_index(definition.id, b)?;
            let recipe = directory.recipe(site.recipe, b)?;
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge,
            } = site.at
            else {
                return Err(fail("executable program retains a Core emission site"));
            };
            if !emitted.insert((
                site.instance,
                site.recipe,
                block.0,
                instruction,
                edge == SemanticSiteEdge::After,
            )) {
                return Err(fail("actual emission boundary has competing ghost actions"));
            }
            b.spend(function.blocks.len().saturating_add(1), WORK)?;
            let block = function
                .blocks
                .iter()
                .find(|candidate| candidate.id == block)
                .ok_or_else(|| fail("emission site names an absent actual block"))?;
            let actual = block.instructions.get(instruction as usize);
            if actual.is_none()
                && !(instruction as usize == block.instructions.len()
                    && edge == SemanticSiteEdge::Before
                    && matches!(site.action, SemanticSiteAction::TypeUse { .. }))
            {
                return Err(fail(
                    "emission site is outside its actual instruction boundary",
                ));
            }
            match &site.action {
                SemanticSiteAction::Construct { value, .. } => Self::resolved(value, b)?,
                SemanticSiteAction::TypeUse { expected, .. }
                | SemanticSiteAction::DeclareLocal { expected, .. } => Self::resolved(expected, b)?,
                SemanticSiteAction::Call { actuals, .. }
                | SemanticSiteAction::BodyCall { actuals } => Self::actuals(actuals, b)?,
                SemanticSiteAction::CoreBodyCall => {
                    return Err(fail(
                        "executable semantic site retains an unresolved Core call",
                    ));
                }
                SemanticSiteAction::Closure { environment, .. } => Self::actuals(environment, b)?,
                SemanticSiteAction::CallableCall { .. } => {}
            }
            match (&recipe.kind, &site.action) {
                (SemanticSiteRecipeKind::OriginalCall, SemanticSiteAction::BodyCall { .. }) => {
                    // An original source call may lower to a finite intrinsic,
                    // constructor or resource operation rather than a Call.
                    // This pass checks its emission boundary, not its scheme.
                    // The original resolver and closed operation consumer below
                    // must still prove the exact opcode, whole P/R, modes,
                    // operands and source/cleanup contracts for this one node.
                    if actual.is_none() || edge != SemanticSiteEdge::After {
                        return Err(fail(
                            "original body call has no actual operation completion boundary",
                        ));
                    }
                }

                (
                    SemanticSiteRecipeKind::Construct { .. },
                    SemanticSiteAction::Construct {
                        result: SemanticOperand::Value(result),
                        ..
                    },
                ) => {
                    if actual.and_then(|instruction| instruction.result) != Some(*result) {
                        return Err(fail("constructor site changes its actual result SSA"));
                    }
                }
                (SemanticSiteRecipeKind::Call { .. }, SemanticSiteAction::Call { target, .. }) => {
                    let callee = self.function(target, b)?;
                    if !matches!(actual.map(|instruction| &instruction.kind), Some(InstructionKind::Call { function, .. } | InstructionKind::SuspendCall { function, .. } | InstructionKind::ForeignCall { function, .. }) if *function == callee.id)
                    {
                        return Err(fail("call site changes the actual called definition"));
                    }
                }
                (
                    SemanticSiteRecipeKind::DeclareLocal { .. },
                    SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(local),
                        ..
                    },
                ) => {
                    if !matches!(actual.map(|instruction| &instruction.kind), Some(InstructionKind::StorageLive(actual)) if actual == local)
                    {
                        return Err(fail(
                            "local declaration does not name its actual StorageLive boundary",
                        ));
                    }
                }
                (SemanticSiteRecipeKind::TypeUse { .. }, SemanticSiteAction::TypeUse { .. }) => {}
                (
                    SemanticSiteRecipeKind::CallableCall { .. },
                    SemanticSiteAction::CallableCall { callee },
                ) => {
                    let matches = match (actual.map(|instruction| &instruction.kind), callee) {
                        (
                            Some(
                                InstructionKind::CallClosureShared { callee: actual, .. }
                                | InstructionKind::CallClosureMut { callee: actual, .. },
                            ),
                            SemanticOperand::Local(callee),
                        ) => actual == callee,
                        (
                            Some(
                                InstructionKind::CallIndirect { callee: actual, .. }
                                | InstructionKind::CallClosure { callee: actual, .. }
                                | InstructionKind::CallClosureOnce { callee: actual, .. }
                                | InstructionKind::CallClosureBorrowed { callee: actual, .. },
                            ),
                            SemanticOperand::Value(callee),
                        ) => actual == callee,
                        _ => false,
                    };
                    if !matches {
                        return Err(fail("callable site changes its actual callee operand"));
                    }
                }
                (
                    SemanticSiteRecipeKind::Closure { .. },
                    SemanticSiteAction::Closure {
                        result: SemanticOperand::Value(result),
                        closure: SemanticClosureTarget::Typed(closure),
                        ..
                    },
                ) => {
                    if actual.and_then(|instruction| instruction.result) != Some(*result)
                        || !matches!(function.values.get(result.0 as usize), Some(Type::Closure(actual)) if actual == closure)
                    {
                        return Err(fail("closure site changes its actual environment identity"));
                    }
                }
                _ => return Err(fail("emission action changes its original recipe kind")),
            }
        }
        Ok(())
    }
}

/// Validate the exact original inventory, then discharge the complete supported
/// instance and operation obligations. Persistent domains remain unknown until
/// their original class relation is interpreted; an empty claim is not a lane.
pub(super) fn pending(
    module: &TypedModule,
    program: &SemanticModule,
    limits: &zil_base::Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    let inventory = Inventory::new(module, limits, program, b)?;
    for definition in &program.definitions {
        b.spend(1, WORK)?;
        if matches!(definition.body, SemanticDefinitionBody::Function { .. }) {
            return Err(vec![zil_base::Diagnostic::unsupported(
                zil_base::Phase::Analyze,
                "original semantic function omits its complete body and value binders",
                None,
            )]);
        }
    }
    inventory.profile_stage("inventory parents", b);
    inventory.parents(program, b).map_err(|errors| inventory.profile_errors(errors, b))?;
    inventory.profile_stage("inventory sites", b);
    inventory.sites(program, b).map_err(|errors| inventory.profile_errors(errors, b))?;
    inventory.profile_stage("original definitions", b);
    let original = original::check(module, program, &inventory, b).map_err(|errors| inventory.profile_errors(errors, b))?;
    closed::check(module, program, &inventory, &original, b).map_err(|errors| inventory.profile_errors(errors, b))
}
