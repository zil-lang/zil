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
// Only closed unpacked scalar literal structures are in the frozen profile.
fn layout(t: &m::Ty, left: &mut usize, depth: usize) -> CompileResult<(u64, u32)> {
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
        m::Ty::Struct(fields) => {
            spend(left, fields.len())?;
            let (mut size, mut alignment) = (0, 1);
            for field in fields {
                let (s, a) = layout(field, left, depth + 1)?;
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
                types.insert(name.as_str(), fields);
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
                let (s, a) = layout(t, left, 0)?;
                size = align(size, a)?;
                spend(left, 16)?; // scalar type rendering and bounded field metadata
                t.hash(&mut hash);
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
