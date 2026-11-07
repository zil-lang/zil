//! Physical SABI v1 artifact, derived only from checked machine output.
use crate::BuildOptions;
use std::hash::Hash;
use zil_base::{CompileResult, Diagnostic, Fingerprint, FingerprintBuilder, Phase, json_string};
use zil_ir::FunctionId;
use zil_llvm_api::model as m;
use zil_verify::VerifiedModule;
#[path = "task_abi_identity.rs"]
mod identity;

pub const TASK_DATA_LAYOUT: &str =
    "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskFrameField {
    pub index: u32,
    pub offset: u64,
    pub size: u64,
    pub alignment: u32,
    pub machine_type: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskAbiEntry {
    pub operation: String,
    pub symbol: String,
    pub result: String,
    pub parameters: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskAbiArtifact {
    pub module: zil_base::ModuleId,
    pub name: String,
    pub worker: FunctionId,
    pub abi_version: u32,
    pub verified_input_fingerprint: Fingerprint,
    pub target_fingerprint: Fingerprint,
    pub codegen_key: Fingerprint,
    pub target: String,
    pub data_layout: String,
    pub frame_size: u64,
    pub frame_alignment: u32,
    pub frame_fields: Vec<TaskFrameField>,
    pub frame_layout_fingerprint: Fingerprint,
    pub entrypoints: Vec<TaskAbiEntry>,
}
impl TaskAbiArtifact {
    pub fn to_json(&self) -> String {
        format!("{{\"module\":{},\"name\":{},\"worker\":{},\"abi_version\":{},\"verified_input_fingerprint\":{},\"target_fingerprint\":{},\"codegen_key\":{},\"target\":{},\"data_layout\":{},\"frame_size\":{},\"frame_alignment\":{},\"frame_layout_fingerprint\":{},\"frame_fields\":[{}],\"entrypoints\":[{}]}}",
            self.module.0,json_string(&self.name),self.worker.0,self.abi_version,json_string(&self.verified_input_fingerprint.to_hex()),json_string(&self.target_fingerprint.to_hex()),json_string(&self.codegen_key.to_hex()),json_string(&self.target),json_string(&self.data_layout),self.frame_size,self.frame_alignment,json_string(&self.frame_layout_fingerprint.to_hex()),
            self.frame_fields.iter().map(|f| format!("{{\"index\":{},\"offset\":{},\"size\":{},\"alignment\":{},\"machine_type\":{}}}",f.index,f.offset,f.size,f.alignment,json_string(&f.machine_type))).collect::<Vec<_>>().join(","),
            self.entrypoints.iter().map(|e| format!("{{\"operation\":{},\"symbol\":{},\"result\":{},\"parameters\":[{}]}}",json_string(&e.operation),json_string(&e.symbol),json_string(&e.result),e.parameters.iter().map(|p|json_string(p)).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(","))
    }
}
fn bad(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Codegen,
        "E-CODEGEN-TASK-ABI",
        None,
        message,
    )]
}
fn spend(left: &mut usize, n: usize) -> CompileResult<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "task ABI artifact budget exceeded",
            None,
        )]
    })?;
    Ok(())
}
fn align(n: u64, a: u32) -> CompileResult<u64> {
    let a = u64::from(a);
    n.checked_add(a - 1)
        .map(|n| n / a * a)
        .ok_or_else(|| bad("task frame layout size overflow"))
}
// This is the explicit LLVM23 Linux x64 target layout, never Rust host layout.
// Named structures resolve only through actual machine definitions to the same
// closed unpacked scalar profile; missing/recursive definitions never grant layout.
type Definitions<'a> = std::collections::BTreeMap<&'a str, &'a Vec<m::Ty>>;
fn layout<'a>(
    t: &'a m::Ty,
    types: &Definitions<'a>,
    active: &mut std::collections::BTreeSet<&'a str>,
    left: &mut usize,
    depth: usize,
) -> CompileResult<(u64, u32)> {
    spend(left, 16)?;
    if depth > 64 {
        return Err(bad(
            "task frame machine type depth exceeds supported profile",
        ));
    }
    Ok(match t {
        m::Ty::Ptr => (8, 8),
        m::Ty::Int(1 | 8) => (1, 1),
        m::Ty::Int(16) => (2, 2),
        m::Ty::Int(32) => (4, 4),
        m::Ty::Int(64) => (8, 8),
        m::Ty::Named(name) => {
            spend(
                left,
                name.len().saturating_add(types.len()).saturating_add(1),
            )?;
            spend(left, active.len().saturating_add(1))?;
            if !active.insert(name.as_str()) {
                return Err(bad("task frame named machine type is recursively inline"));
            }
            let fields = types
                .get(name.as_str())
                .ok_or_else(|| bad("task frame named machine type has no actual definition"))?;
            spend(left, fields.len())?;
            let (mut size, mut alignment) = (0, 1);
            for field in *fields {
                let (s, a) = layout(field, types, active, left, depth + 1)?;
                size = align(size, a)?
                    .checked_add(s)
                    .ok_or_else(|| bad("task frame size overflow"))?;
                alignment = alignment.max(a);
            }
            active.remove(name.as_str());
            (align(size, alignment)?, alignment)
        }
        m::Ty::Struct(fields) => {
            spend(left, fields.len())?;
            let (mut size, mut alignment) = (0, 1);
            for field in fields {
                let (s, a) = layout(field, types, active, left, depth + 1)?;
                size = align(size, a)?
                    .checked_add(s)
                    .ok_or_else(|| bad("task frame size overflow"))?;
                alignment = alignment.max(a);
            }
            (align(size, alignment)?, alignment)
        }
        _ => {
            return Err(bad(
                "task frame contains an unsupported target machine type",
            ));
        }
    })
}
// Embedded child structures make one outer field recursively large. Reserve
// every actual hash node and rendered byte before Hash/Display visits it.
fn reserve_field_identity<'a>(
    t: &'a m::Ty,
    types: &Definitions<'a>,
    named: &mut Vec<(&'a str, &'a Vec<m::Ty>)>,
    left: &mut usize,
    depth: usize,
) -> CompileResult<()> {
    spend(left, 8)?;
    if depth > 64 {
        return Err(bad("task field identity exceeds supported depth"));
    }
    match t {
        m::Ty::Ptr => spend(left, 3),
        m::Ty::Int(1 | 8) => spend(left, 2),
        m::Ty::Int(16 | 32 | 64) => spend(left, 3),
        m::Ty::Named(name) => {
            spend(
                left,
                name.len()
                    .saturating_mul(4)
                    .saturating_add(types.len())
                    .saturating_add(8),
            )?;
            let fields = *types
                .get(name.as_str())
                .ok_or_else(|| bad("task field named identity has no actual definition"))?;
            spend(left, fields.len().saturating_mul(2).saturating_add(4))?;
            named.push((name, fields));
            for field in fields {
                reserve_field_identity(field, types, named, left, depth + 1)?;
            }
            Ok(())
        }
        m::Ty::Struct(fields) => {
            spend(left, fields.len().saturating_mul(2).saturating_add(4))?;
            for field in fields {
                reserve_field_identity(field, types, named, left, depth + 1)?;
            }
            Ok(())
        }
        _ => Err(bad(
            "task field identity contains unsupported physical type",
        )),
    }
}
fn field_identity(
    t: &m::Ty,
    types: &Definitions<'_>,
    hash: &mut FingerprintBuilder,
    left: &mut usize,
) -> CompileResult<()> {
    let mut named = vec![];
    reserve_field_identity(t, types, &mut named, left, 0)?;
    t.hash(hash);
    for (name, definition) in named {
        hash.update(b"ZIL-TASK-ACTUAL-NAMED-FIELD\0");
        name.hash(hash);
        definition.hash(hash);
    }
    Ok(())
}
pub(crate) fn derive(
    verified: &VerifiedModule,
    machine: &m::Module,
    left: &mut usize,
) -> CompileResult<Vec<TaskAbiArtifact>> {
    if verified.task_interfaces().is_empty() {
        return Ok(vec![]);
    }
    if machine.triple != "x86_64-unknown-linux-gnu"
        || verified.module().target.pointer_width != 64
        || verified.module().target.endian != zil_base::Endian::Little
    {
        return Err(bad(
            "public task ABI layout supports only the frozen Linux x64 little-endian target",
        ));
    }
    spend(left, machine.items.len().saturating_mul(2))?;
    let mut types = std::collections::BTreeMap::new();
    let mut functions = std::collections::BTreeMap::new();
    for item in &machine.items {
        match item {
            m::Item::NamedType(name, fields) => {
                spend(left, name.len().saturating_add(types.len()))?;
                if types.insert(name.as_str(), fields).is_some() {
                    return Err(bad(
                        "task machine named type has duplicate actual definitions",
                    ));
                }
            }
            m::Item::Function(f) => {
                spend(left, f.name.len().saturating_add(functions.len()))?;
                functions.insert(f.name.as_str(), f);
            }
            _ => {}
        }
    }
    let mut result = vec![];
    for interface in verified.task_interfaces() {
        for export in &interface.exports {
            spend(left, export.name.len().saturating_add(6))?;
            let frame_name = format!("zil_task_frame_{}", export.worker.0);
            let fields = types
                .get(frame_name.as_str())
                .ok_or_else(|| bad("exported task frame is absent from actual machine output"))?;
            spend(left, fields.len())?;
            let mut hash = FingerprintBuilder::new();
            hash.update(b"ZIL-TASK-FRAME-LAYOUT-v1\0");
            hash.update(TASK_DATA_LAYOUT.as_bytes());
            let (mut size, mut alignment) = (0u64, 1u32);
            let mut actual = vec![];
            for (index, t) in fields.iter().enumerate() {
                let (s, a) = layout(t, &types, &mut std::collections::BTreeSet::new(), left, 0)?;
                size = align(size, a)?;
                spend(left, 16)?;
                field_identity(t, &types, &mut hash, left)?;
                size.hash(&mut hash);
                s.hash(&mut hash);
                a.hash(&mut hash);
                actual.push(TaskFrameField {
                    index: index as u32,
                    offset: size,
                    size: s,
                    alignment: a,
                    machine_type: t.to_string(),
                });
                size = size
                    .checked_add(s)
                    .ok_or_else(|| bad("task frame size overflow"))?;
                alignment = alignment.max(a);
            }
            size = align(size, alignment)?;
            size.hash(&mut hash);
            alignment.hash(&mut hash);
            let mut entrypoints = vec![];
            for operation in [
                "init",
                "poll",
                "request_cancel",
                "poll_close",
                "finish_destroy",
            ] {
                spend(left, 64)?;
                let symbol = crate::emit::task_export_name(export.worker, operation);
                let f = functions
                    .get(symbol.as_str())
                    .ok_or_else(|| bad("task SABI export missing from actual machine output"))?;
                if f.linkage != m::Linkage::External || f.blocks.is_none() {
                    return Err(bad("task SABI entry is not an external machine definition"));
                }
                let (param, ret) = match operation {
                    "init" => (m::Ty::Int(32), m::Ty::Ptr),
                    "poll" => (m::Ty::Ptr, m::Ty::Int(64)),
                    "poll_close" => (m::Ty::Ptr, m::Ty::Int(1)),
                    _ => (m::Ty::Ptr, m::Ty::Void),
                };
                if f.signature.result != ret
                    || f.signature.params.len() != 1
                    || f.signature.params[0].ty != param
                    || f.signature.variadic
                {
                    return Err(bad("actual task SABI signature differs from scalar v1"));
                }
                entrypoints.push(TaskAbiEntry {
                    operation: operation.into(),
                    symbol,
                    result: ret.to_string(),
                    parameters: vec![param.to_string()],
                });
            }
            result.push(TaskAbiArtifact {
                module: interface.module,
                name: export.name.clone(),
                worker: export.worker,
                abi_version: export.abi_version,
                verified_input_fingerprint: verified.task_abi_input_fingerprint(),
                target_fingerprint: verified.module().target.fingerprint(),
                codegen_key: Fingerprint::ZERO,
                target: machine.triple.clone(),
                data_layout: TASK_DATA_LAYOUT.into(),
                frame_size: size,
                frame_alignment: alignment,
                frame_fields: actual,
                frame_layout_fingerprint: hash.digest(),
                entrypoints,
            });
        }
    }
    Ok(result)
}

pub(crate) fn bind_codegen(
    artifacts: &mut [TaskAbiArtifact],
    units: &[m::Module],
    options: &BuildOptions,
    left: &mut usize,
) -> CompileResult<()> {
    if artifacts.is_empty() {
        return Ok(());
    }
    let mut hash = FingerprintBuilder::new();
    hash.update(b"ZIL-TASK-CODEGEN-ACTION-v1\0");
    for unit in units {
        identity::prepay(unit, left)?;
        unit.hash(&mut hash);
    }
    spend(
        left,
        options
            .clang
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .saturating_mul(4)
            .saturating_add(256),
    )?;
    format!(
        "{:?}|{:?}|{:?}|{}|{}|{}|{}|{:?}",
        options.backend,
        options.optimization,
        options.function_merging,
        options.codegen.units,
        options.codegen.jobs,
        options.closed_executable,
        options.incremental_o2,
        options.clang
    )
    .hash(&mut hash);
    crate::installed_compiler_identity()
        .map_err(|_| bad("cannot bind compiler identity for task ABI artifact"))?
        .hash(&mut hash);
    if options.backend == crate::NativeBackend::LlvmApi {
        let config = zil_llvm_api::Config {
            triple: &artifacts[0].target,
            optimization: match options.optimization {
                crate::Optimization::O0 => zil_llvm_api::Optimization::O0,
                crate::Optimization::O2 => zil_llvm_api::Optimization::O2,
            },
        };
        let (version, library) = zil_llvm_api::cache_runtime(&config)
            .map_err(|_| bad("cannot bind actual LLVM runtime for task ABI artifact"))?;
        spend(left, version.len().saturating_mul(2).saturating_add(64))?;
        version.hash(&mut hash);
        crate::object_cache::installed_identity(&library)
            .map_err(|_| bad("cannot identify actual LLVM runtime for task ABI artifact"))?
            .hash(&mut hash);
    } else {
        let tool = if options.clang.components().count() > 1 {
            Some(options.clang.clone())
        } else {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|path| path.join(&options.clang))
                    .find(|path| path.is_file())
            })
        }
        .ok_or_else(|| bad("cannot resolve actual Clang for task ABI artifact"))?;
        crate::object_cache::installed_identity(&tool)
            .map_err(|_| bad("cannot identify actual Clang for task ABI artifact"))?
            .hash(&mut hash);
    }
    let key = hash.digest();
    for artifact in artifacts {
        artifact.codegen_key = key;
    }
    Ok(())
}

pub(crate) fn prepay_owned_frame(function: &m::Function, left: &mut usize) -> CompileResult<()> {
    identity::prepay_function(function, left)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_frame_layout_resolves_actual_definitions_and_charges_identity() {
        let top = m::Ty::Named("environment".into());
        let first = vec![m::Ty::Int(32), m::Ty::Int(1)];
        let second = vec![m::Ty::Int(1), m::Ty::Int(32)];
        let run = |fields: &Vec<m::Ty>, steps| {
            let types = Definitions::from([("environment", fields)]);
            let mut left = steps;
            let layout = layout(&top, &types, &mut Default::default(), &mut left, 0)?;
            let mut hash = FingerprintBuilder::new();
            field_identity(&top, &types, &mut hash, &mut left)?;
            Ok::<_, Vec<Diagnostic>>((layout, hash.digest(), steps - left))
        };
        let a = run(&first, 10000).unwrap();
        let b = run(&second, 10000).unwrap();
        assert_eq!(a.0, (8, 4));
        assert_eq!(a.0, b.0);
        assert_ne!(
            a.1, b.1,
            "identical names/size cannot hide different actual inline fields"
        );
        assert_eq!(run(&first, a.2).unwrap(), a);
        assert_eq!(
            run(&first, a.2 - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(run(&first, a.2).unwrap(), a);
        let empty = Definitions::new();
        assert!(
            layout(&top, &empty, &mut Default::default(), &mut 10000, 0).unwrap_err()[0]
                .message
                .contains("no actual definition")
        );
        assert_eq!(
            layout(&m::Ty::Ptr, &empty, &mut Default::default(), &mut 10000, 0).unwrap(),
            (8, 8),
            "opaque pointers never chase an invented pointee"
        );
        let cycle = vec![top.clone()];
        let types = Definitions::from([("environment", &cycle)]);
        assert!(
            layout(&top, &types, &mut Default::default(), &mut 10000, 0).unwrap_err()[0]
                .message
                .contains("recursively inline")
        );
        let mut deep = m::Ty::Int(32);
        for _ in 0..65 {
            deep = m::Ty::Struct(vec![deep]);
        }
        let deep = vec![deep];
        let types = Definitions::from([("environment", &deep)]);
        assert!(
            layout(&top, &types, &mut Default::default(), &mut 10000, 0).unwrap_err()[0]
                .message
                .contains("depth")
        );
    }
}
