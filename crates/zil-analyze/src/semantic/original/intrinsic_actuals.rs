//! The sealed operation's type argument is derived from its checked original
//! scheme. It is separate from written generic function application arguments.
use super::*;

impl Checked {
    pub(in crate::semantic) fn intrinsic_type_actual(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<crate::semantic::value::Value> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("sealed Type actual has no complete original body"));
        };
        let mut node = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(expression) = link.node else {
                return Err(fail("sealed Type actual selects an original statement"));
            };
            if node.replace(expression).is_some() {
                return Err(fail("sealed Type actual has competing original nodes"));
            }
        }
        let node = node.ok_or_else(|| fail("sealed Type actual omits its original call"))?;
        b.lookup(self.intrinsics.len(), WORK)?;
        let intrinsic = *self
            .intrinsics
            .get(&(context.definition, node))
            .ok_or_else(|| fail("sealed Type actual has no independently checked scheme"))?;
        b.lookup(self.intrinsic_inputs.len(), WORK)?;
        let inputs = self
            .intrinsic_inputs
            .get(&(context.definition, node))
            .ok_or_else(|| fail("sealed Type actual omits its complete original inputs"))?;
        use sealed::Intrinsic as I;
        let registration = matches!(
            intrinsic,
            I::RegistrationValid
                | I::RegistrationClose
                | I::AsyncValid
                | I::AsyncRequest
                | I::AsyncPoll
                | I::AsyncFinish
                | I::AsyncFinishChecked
        );
        let (term, expect_task) = match intrinsic {
            I::StoreNew => (
                *inputs
                    .first()
                    .ok_or_else(|| fail("original Store constructor has no element Type"))?,
                false,
            ),
            I::StoreReserve
            | I::StorePush
            | I::StorePop
            | I::StoreLen
            | I::StoreCapacity
            | I::StoreAt
            | I::StoreAtMut => (
                *inputs
                    .get(1)
                    .ok_or_else(|| fail("original Store scheme has no selected manager"))?,
                false,
            ),
            I::TaskCreate => {
                let frame = self
                    .frames
                    .get(&context.definition)
                    .ok_or_else(|| fail("original Task constructor has no checked frame"))?;
                b.lookup(frame.expressions.len(), WORK)?;
                (
                    *frame
                        .expressions
                        .get(&node)
                        .ok_or_else(|| fail("original Task constructor has no complete result"))?,
                    true,
                )
            }
            I::TaskPoll | I::TaskPollClose | I::TaskCancel | I::TaskFinish => (
                *inputs
                    .first()
                    .ok_or_else(|| fail("original Task operation has no selected head"))?,
                true,
            ),
            I::RegistrationValid
            | I::RegistrationClose
            | I::AsyncValid
            | I::AsyncRequest
            | I::AsyncPoll
            | I::AsyncFinish
            | I::AsyncFinishChecked => (
                *inputs
                    .first()
                    .ok_or_else(|| fail("original registration operation has no complete head"))?,
                false,
            ),
            _ => {
                return Err(fail(
                    "operation has no sealed Store/Task Type argument scheme",
                ));
            }
        };
        if intrinsic == I::StoreNew {
            return self.value(term, context, program, inventory, b, 0);
        }
        let mut current = term;
        for _ in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            current = self.root(current, b)?;
            current = match &self.arena[current] {
                Kind::CallResult { hidden: inner, .. }
                | Kind::Address { inner, .. }
                | Kind::Desc { inner, .. }
                | Kind::Wrapped(Wrapper::Managed, inner) => *inner,
                Kind::ErrorUnion { result, .. } if expect_task => *result,
                Kind::Wrapped(Wrapper::Registration, environment)
                | Kind::Wrapped(Wrapper::AsyncRegistration, environment)
                    if registration =>
                {
                    return self.value(*environment, context, program, inventory, b, 0);
                }
                Kind::Wrapped(Wrapper::Store, element) if !expect_task => {
                    return self.value(*element, context, program, inventory, b, 0);
                }
                Kind::Task(task) if expect_task => {
                    return self.value(task.result, context, program, inventory, b, 0);
                }
                _ => {
                    return Err(fail(
                        "sealed Type actual changes the complete original resource family",
                    ));
                }
            };
        }
        Err(resource(
            "sealed Type actual exceeds its bounded original wrapper depth",
        ))
    }
}
