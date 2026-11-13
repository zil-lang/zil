//! Complete original signature substitution. The result is an execution
//! obligation, never a successful call, access or cleanup certificate.
use super::*;

pub(super) struct Resolved {
    pub parameters: Vec<terms::Value>,
    pub parameter_modes: Vec<PassingMode>,
    pub result: terms::Value,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
    pub contract: DynContract,
    pub operands: Vec<requests::Operand>,
}

pub(super) fn materialize(
    module: &TypedModule,
    recipe: &DynSignatureRecipe,
    scope: DynInstantiationScope,
    mut resolve: impl FnMut(&DynTypePattern, &mut Budget) -> Check<terms::Value>,
    b: &mut Budget,
) -> Check<Resolved> {
    dynamic_metadata::recipe(recipe, b)?;
    if recipe.invocation_binder_count > scope.invocation_binder_count {
        return Err(bad(
            "original method invocation domain is outside its actual request",
        ));
    }
    b.spend(recipe.parameters.len())?;
    let mut parameters = Vec::with_capacity(recipe.parameters.len());
    let mut parameter_modes = Vec::with_capacity(recipe.parameters.len());
    for parameter in &recipe.parameters {
        let value = resolve(&parameter.ty, b)?;
        values::check(module, &value.raw, scope, b)?;
        parameters.push(value);
        parameter_modes.push(parameter.mode);
    }
    let result = resolve(&recipe.result, b)?;
    values::check(module, &result.raw, scope, b)?;
    let (contract, operands) = contracts::materialize(&recipe.contract, &mut resolve, b)?;
    for operand in &operands {
        values::check(module, &operand.value, scope, b)?;
    }
    Ok(Resolved {
        parameters,
        parameter_modes,
        result,
        result_mode: recipe.result_mode,
        call_kind: recipe.call_kind,
        contract,
        operands,
    })
}
impl Resolved {
    /// Header comparisons preserve the actual complete contract (including
    /// Static occurrences). Its implementation still needs an actual CFG proof.
    pub(super) fn function(
        &self,
        module: &TypedModule,
        actual: &TypedFunction,
        b: &mut Budget,
    ) -> Check<(requests::Header, Vec<DynLifeEmbedding>)> {
        if self.result_mode != actual.result_mode || self.call_kind != actual.call_kind {
            return Err(bad(
                "actual entry changes original result responsibility or call kind",
            ));
        }
        headers::rebased_function(module, actual, &self.parameters, &self.result, b)
    }
    pub(super) fn same_signature(&self, other: &Self, b: &mut Budget) -> Check<()> {
        if self.parameter_modes != other.parameter_modes
            || self.result_mode != other.result_mode
            || self.call_kind != other.call_kind
            || self.parameters.len() != other.parameters.len()
        {
            return Err(bad(
                "implementation changes the original trait method roles",
            ));
        }
        for (a, z) in self
            .parameters
            .iter()
            .zip(&other.parameters)
            .chain(std::iter::once((&self.result, &other.result)))
        {
            if !external::terms_equal(a, z, b)? {
                return Err(bad(
                    "implementation changes a complete original method type or origin",
                ));
            }
        }
        Ok(())
    }
}

/// Explicit lexical -> request images, reconstructed from the whole selected
/// specialization and every header occurrence. It is deliberately not an
/// inverse map, and says nothing about intrinsic origin equality.
#[derive(Default)]
struct Images(Vec<DynLifeEmbedding>);
impl Images {
    fn bind(&mut self, from: StorageBinder, to: StorageBinder, b: &mut Budget) -> Check<()> {
        b.spend(self.0.len().saturating_add(1))?;
        if let Some(row) = self.0.iter().find(|row| row.from == from) {
            if row.to != to {
                return Err(bad(
                    "selected specialization has inconsistent outside images",
                ));
            }
        } else {
            self.0.push(DynLifeEmbedding { from, to });
        }
        Ok(())
    }
    fn value(&mut self, from: &DynTypeValue, to: &DynTypeValue, b: &mut Budget) -> Check<()> {
        let map = external::alignment(from, to, b)?
            .ok_or_else(|| bad("selected entry has a different full intrinsic type"))?;
        if from.classes.len() != map.len() || to.classes.len() != map.len() {
            return Err(bad("selected entry loses a complete intrinsic class"));
        }
        for (i, j) in map.into_iter().enumerate() {
            self.bind(from.classes[i], to.classes[j as usize], b)?;
        }
        Ok(())
    }
    fn actuals(
        &mut self,
        module: &TypedModule,
        from: &[DynGenericActual],
        to: &[DynGenericActual],
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(from.len().saturating_add(to.len()))?;
        if from.len() != to.len() {
            return Err(bad("entry changes original complete actual arity"));
        }
        for (from, to) in from.iter().zip(to) {
            dynamic_metadata::recipe(from, b)?;
            dynamic_metadata::recipe(to, b)?;
            match (from, to) {
                (DynGenericActual::Life(a), DynGenericActual::Life(z)) => self.bind(*a, *z, b)?,
                (DynGenericActual::QualifiedType(a), DynGenericActual::QualifiedType(z)) => {
                    self.value(a, z, b)?
                }
                (DynGenericActual::Type(a), DynGenericActual::Type(z)) if a == z => {
                    values::closed(module, a, b)?
                }
                (DynGenericActual::Type(a), DynGenericActual::QualifiedType(z))
                | (DynGenericActual::QualifiedType(z), DynGenericActual::Type(a))
                    if a == &z.ty && z.life.binder_count == 0 && z.classes.is_empty() =>
                {
                    values::closed(module, a, b)?
                }
                (DynGenericActual::Const(a), DynGenericActual::Const(z)) if a == z => {}
                _ => return Err(bad("entry changes an original full actual kind or value")),
            }
        }
        Ok(())
    }
}

pub(super) fn specialization(
    module: &TypedModule,
    function: &TypedFunction,
    definition: DynDefinitionId,
    receiver: &terms::Value,
    declaration_actuals: &[DynGenericActual],
    method_actuals: &[DynGenericActual],
    header: &[DynLifeEmbedding],
    b: &mut Budget,
) -> Check<Vec<DynLifeEmbedding>> {
    let instance = function
        .dyn_instance
        .as_ref()
        .ok_or_else(|| bad("method entry original specialization is Unknown"))?;
    if instance.definition != definition {
        return Err(bad("method entry selects another original definition"));
    }
    scopes::function(module, function, instance, b)?;
    let mut images = Images::default();
    images.value(&instance.receiver, &receiver.raw, b)?;
    images.actuals(
        module,
        &instance.declaration_actuals,
        declaration_actuals,
        b,
    )?;
    images.actuals(module, &instance.method_actuals, method_actuals, b)?;
    for row in header {
        b.spend(instance.header_embedding.len())?;
        let actual = instance
            .header_embedding
            .iter()
            .find(|actual| actual.from == row.from)
            .ok_or_else(|| bad("method header lacks its actual lexical embedding"))?;
        images.bind(actual.to, row.to, b)?;
    }
    let count = instance.scope.fixed_binder_count as usize
        + instance.scope.invocation_binder_count as usize;
    if count > b.limits.max_nodes {
        return Err(limit(
            "method lexical image inventory exceeds selected bound",
        ));
    }
    for binder in (0..instance.scope.fixed_binder_count)
        .map(StorageBinder::Fixed)
        .chain((0..instance.scope.invocation_binder_count).map(StorageBinder::Invocation))
    {
        b.spend(images.0.len())?;
        if !images.0.iter().any(|row| row.from == binder) {
            return Err(bad(
                "method lexical class is not anchored by its complete specialization or header",
            ));
        }
    }
    Ok(images.0)
}

/// Not a ContractRequest yet: physical/logical Self leaves and actual CFG
/// consumers must be joined before this can discharge any pending call.
pub(super) struct MethodObligation {
    pub origin: requests::Origin,
    pub function: FunctionId,
    pub signature: Resolved,
    pub expected_header: requests::Header,
    pub header_embedding: Vec<DynLifeEmbedding>,
    pub lexical_images: Vec<DynLifeEmbedding>,
}
