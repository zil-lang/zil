//! Exact, reversible-field encoding of the admitted primitive context subset.
//! Nonempty ownership/interface domains decline raw disk emission reuse.
use super::*;
use crate::emit::raw_codec::Writer;

impl Session {
    pub(in crate::emit) fn raw_context(&self) -> Option<Vec<u8>> {
        let Context {
            target,
            limits,
            entry,
            scalar_policy,
            headers,
            globals,
        } = self.context.as_deref()?;
        let Globals {
            schema,
            module,
            resources,
            closures,
            dyn_declarations,
            dyn_interfaces,
            dyn_vtables,
            life_declarations,
            life_instances,
            task_interfaces,
            workers,
            roles,
            flags,
        } = globals;
        if !resources.is_empty()
            || !closures.is_empty()
            || !dyn_declarations.is_empty()
            || !dyn_interfaces.is_empty()
            || !dyn_vtables.is_empty()
            || !life_declarations.is_empty()
            || !life_instances.is_empty()
            || !task_interfaces.is_empty()
            || !workers.is_empty()
            || flags.iter().any(|flag| *flag)
            || roles.iter().any(|(_, a, b, c)| *a || *b || *c)
        {
            return None;
        }
        let mut out = Writer::bounded(limits.max_source_bytes);
        out.put(b"ZIL-RAW-EMISSION-CONTEXT-1")?;
        let TargetProfile {
            id,
            triple,
            pointer_width,
            endian,
            backend,
            rule_version,
            checked_arithmetic,
        } = target;
        for value in [id, triple, backend, rule_version] {
            out.text(value)?;
        }
        out.word(*pointer_width as usize)?;
        out.word(match endian {
            zil_base::Endian::Little => 0,
            zil_base::Endian::Big => 1,
        })?;
        out.word(usize::from(*checked_arithmetic))?;
        let zil_base::Limits {
            max_source_bytes,
            max_tokens,
            max_nodes,
            max_functions,
            max_blocks,
            max_instructions,
            max_locals,
            max_values,
            max_analysis_steps,
            max_ct_steps,
            max_call_depth,
        } = limits;
        for value in [
            max_source_bytes,
            max_tokens,
            max_nodes,
            max_functions,
            max_blocks,
            max_instructions,
            max_locals,
            max_values,
            max_analysis_steps,
            max_call_depth,
        ] {
            out.put(&u64::try_from(*value).ok()?.to_le_bytes())?;
        }
        out.put(&max_ct_steps.to_le_bytes())?;
        match entry {
            EntryPoint::Main => out.word(0)?,
            EntryPoint::None => out.word(1)?,
            EntryPoint::ReturnValue(id) => {
                out.word(2)?;
                out.word(id.0 as usize)?;
            }
            EntryPoint::Test(id) => {
                out.word(3)?;
                out.word(id.0 as usize)?;
            }
        }
        out.word(usize::from(*scalar_policy))?;
        out.word(*schema as usize)?;
        out.word(module.0 as usize)?;
        out.word(roles.len())?;
        for (id, a, b, c) in roles {
            out.word(id.0 as usize)?;
            for flag in [a, b, c] {
                out.word(usize::from(*flag))?;
            }
        }
        out.word(headers.len())?;
        for (key, header) in headers {
            // No synthetic function/locals/CFG or per-header wire allocation.
            // Every admitted Header field has an injective bounded encoding;
            // nonprimitive/ownership domains decline the entire raw cache.
            let Header {
                id,
                name,
                foreign,
                parameters,
                result,
                result_mode,
                file,
                storage_life,
                call_kind,
                reference_return_bound,
                callable_return_contract,
                callable_contract,
                protocol_contract,
                environment,
            } = header;
            if storage_life.is_some()
                || reference_return_bound.is_some()
                || callable_return_contract.is_some()
                || callable_contract.is_some()
                || protocol_contract.is_some()
                || environment.is_some()
            {
                return None;
            }
            out.word(key.0 as usize)?;
            out.word(id.0 as usize)?;
            out.text(name)?;
            out.word(file.0 as usize)?;
            out.word(match call_kind {
                zil_ir::CallKind::Direct => 0,
                zil_ir::CallKind::Suspend => return None,
            })?;
            out.word(match result_mode {
                zil_ir::PassingMode::Copy => 0,
                zil_ir::PassingMode::Move => 1,
                zil_ir::PassingMode::Describe => 2,
                zil_ir::PassingMode::Create => 3,
                zil_ir::PassingMode::Give => 4,
                zil_ir::PassingMode::Borrow => 5,
                zil_ir::PassingMode::BorrowMut => 6,
                zil_ir::PassingMode::Infer => 7,
            })?;
            primitive(&mut out, result)?;
            out.word(parameters.len())?;
            for (id, ty) in parameters {
                out.word(id.0 as usize)?;
                primitive(&mut out, ty)?;
            }
            out.word(usize::from(foreign.is_some()))?;
            if let Some(zil_ir::ForeignDeclaration {
                abi,
                symbol,
                noescape_parameters,
                static_callbacks,
                scoped_callback,
            }) = foreign
            {
                if *static_callbacks || scoped_callback.is_some() {
                    return None;
                }
                out.text(abi)?;
                out.text(symbol)?;
                out.word(noescape_parameters.len())?;
                for index in noescape_parameters {
                    out.word(*index as usize)?;
                }
                // false/None and all other absent domains above are fixed by
                // this protocol, rather than omitted varying input fields.
            }
        }
        Some(out.0)
    }
}

fn primitive(out: &mut Writer, ty: &Type) -> Option<()> {
    out.word(match ty {
        Type::Unit => 0,
        Type::Bool => 1,
        Type::I32 => 2,
        Type::I64 => 3,
        Type::U8 => 4,
        Type::U16 => 5,
        _ => return None,
    })
}
