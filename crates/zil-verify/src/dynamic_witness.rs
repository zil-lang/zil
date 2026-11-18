//! Independent interpretation of original dynamic declarations and witnesses.
//! Metadata traversal supplies bounds only. This module never consumes Analyze
//! summaries, candidate-plan roots, or the producer's obligation solver.
use super::*;
mod compound;
pub(crate) mod constants;
mod constructors;
mod declarations;
mod definitions;
mod graph;
mod implicit;
mod instantiate;
mod keys;
mod matching;
pub(crate) mod semantic_patterns;
mod queries;
mod scopes;
mod structural;
mod terms;
mod values;
pub(crate) use values::active_contains;

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-WITNESS", None, message)
}

/// Establish original scoped grammar and an acyclic declaration-level super
/// graph before actual application substitution. This is only one prerequisite
/// of the implementation proof; it does not authorize a table or method.
pub(super) fn declaration_graph(
    module: &TypedModule,
    budget: &mut Budget,
) -> Check<requests::Pending> {
    declarations::check(module, budget)?;
    definitions::check(module, budget)?;
    graph::claims(module, budget)
}

mod bridge;
mod callables;
mod context_ports;
mod contracts;
mod external;
mod headers;
mod inventory;
pub(super) mod requests;
mod signatures;
mod symbols;

pub(crate) mod semantic_external;
mod semantic_capabilities;

/// Only the complete FnOnce consuming operation can use this explicit upper
/// adapter; ordinary function-pointer/vtable ABI matching stays invariant.
pub(crate) fn consuming_upper_compatible(kind:CallableKind,actual:CallKind,required:CallKind)->bool{callables::consuming_upper_compatible(kind,actual,required)}

/// Complete actual Typed domain only. Pending sources remain pending; neither
/// the cardinality nor successful shape validation grants memory authority.
#[track_caller]
pub(crate) fn intrinsic_domain(
    module: &TypedModule,
    ty: &Type,
    budget: &mut Budget,
) -> Check<(u32, bool)> {
    let domain = values::intrinsic(module, ty, budget)?;
    Ok((domain.count, domain.pending_sources))
}

/// Complete actual inventory and the existing independent structural rule.
/// Pending source/loan/phase obligations remain with their normal consumers.
#[track_caller]
pub(crate) fn structural_actual(module:&TypedModule,ty:&Type,bound:DynStructuralBound,b:&mut Budget)->Check<()> {
    // A structural predicate does not request an outside parent class map.
    // Validate every complete child independently, preserving its own domain;
    // a bare aggregate's zero-domain query is a different obligation.
    let caller=std::panic::Location::caller();
    let result=(||{values::structural_children(module,ty,b)?;structural::actual(module,ty,bound,b)})();
    match result{Ok(())=>Ok(()),Err(error)=>{
        b.spend(caller.file().len().saturating_add(96))?;
        Err(error.with_note(format!("actual structural {bound:?} consumer {}:{}:{}",caller.file(),caller.line(),caller.column())))
    }}
}

/// Reuse the independent full Typed inventory while the semantic context
/// separately validates every class expression and its original scope.
pub(crate) fn check_type_value(
    module: &TypedModule,
    value: &DynTypeValue,
    scope: DynInstantiationScope,
    budget: &mut Budget,
) -> Check<()> {
    values::check(module, value, scope, budget).map(|_| ())
}

/// The same complete inventory over borrowed parts. This avoids constructing
/// an owned DTV solely to run the independent read-only interpreter.
pub(crate) fn check_type_parts(
    module: &TypedModule,
    ty: &Type,
    life: &StorageLifeShape,
    classes: &[StorageBinder],
    scope: DynInstantiationScope,
    budget: &mut Budget,
) -> Check<()> {
    values::check_parts(module,ty,life,classes,scope,budget).map(|_| ())
}

/// Rebuild every physical/retained/owning environment leaf from the exact
/// actual closure header. This is a second consumer of the original request,
/// not a source/loan/cleanup certificate; those remain with the CFG kernels.
pub(crate) fn check_callable_environment(module:&TypedModule,request:&requests::CallableRequest,b:&mut Budget)->Check<()>{
    use requests::{IntrinsicScope as S,IntrinsicRoot as R};
    b.spend(request.receiver.classes.len().saturating_add(request.intrinsic_classes.len()))?;
    if request.receiver.classes.len()!=request.intrinsic_classes.len(){return Err(bad("callable environment omits a whole intrinsic class"));}
    let terms=request.receiver.classes.iter().zip(&request.intrinsic_classes).map(|(image,origin)|terms::LifeTerm{
        image:*image,origin:terms::Origin{
            owner:match origin.scope{S::Semantic(frame)=>terms::Owner::Semantic(frame),S::Function(function)=>terms::Owner::Function(function),S::Witness{table,node}=>terms::Owner::Witness(table,node)},
            root:match origin.root{R::Receiver=>terms::Root::Receiver,R::DeclarationType(index)=>terms::Root::DeclarationType(index),R::MethodType(index)=>terms::Root::MethodType(index),R::ApplicationType(index)=>terms::Root::ApplicationType(index),R::Lexical(binder)=>terms::Root::Lexical(binder),R::Quantified(scope,index)=>terms::Root::Quantified(scope,index),R::Semantic(scope,index)=>terms::Root::Semantic(scope,index)},class:origin.class,
        }
    }).collect();
    let value=terms::Value{raw:request.receiver.clone(),terms};
    let leaves=inventory::self_leaves(module,&value,request.contract.scope,b)?;
    b.spend(leaves.len().saturating_add(request.contract.self_leaves.len()))?;
    if leaves!=request.contract.self_leaves{return Err(bad("callable hidden environment changes an exact actual mode/source/whole-child occurrence"));}Ok(())
}

/// Reuse this kernel's complete Context proof when a concrete slot relates a
/// port's whole pointee domain to a result or another actual argument.
pub(crate) fn context_port_inner(module:&TypedModule,interface:&DynInterface,port:u32,budget:&mut Budget)->Check<Vec<(u32,u32)>>{
    let ports=context_ports::check(module,interface,|ty,b|values::intrinsic(module,ty,b).map(|domain|domain.count),budget)?;
    let row=ports.get(port as usize).ok_or_else(||bad("Context inner-map query selects an absent port"))?;
    budget.spend(row.inner.len().saturating_mul(2).saturating_add(1))?;
    Ok(row.inner.clone())
}
