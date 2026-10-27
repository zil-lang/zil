//! A sealed runtime prefix. Whole elements move as their ordinary inline ABI.
//! No element pointers or mutable head fields escape these checked operations.
use super::*;

pub(super) fn is_resource(ty: &Type, definitions: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::Managed(payload) | Type::Desc(payload)
        if matches!(payload.as_ref(), Type::OwnedStore { .. })
        && definitions.iter().any(|r| r.payload == **payload && r.teardown.is_none()))
}

pub(super) fn helper(
    payload: &Type,
    definitions: &[ResourceDefinition],
    operation: &str,
) -> CompileResult<String> {
    let Some(index) = definitions
        .iter()
        .position(|r| r.payload == *payload && r.teardown.is_none())
    else {
        return invalid(
            "E-CODEGEN-CFG",
            "Store operation has no exact sealed resource definition",
            None,
        );
    };
    if !matches!(payload, Type::OwnedStore { .. }) {
        return invalid(
            "E-CODEGEN-CFG",
            "Store runtime requires its sealed payload",
            None,
        );
    }
    Ok(format!("zil.store.{index}.{operation}"))
}

pub(super) fn runtime(lir: &LirModule) -> CompileResult<String> {
    if !lir
        .resources
        .iter()
        .any(|r| matches!(r.payload, Type::OwnedStore { .. }))
    {
        return Ok(String::new());
    }
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return unsupported("owned Store currently requires Linux x64 LLVM", None);
    }
    if let Some(f) = lir.functions.iter().find(|f| {
        f.foreign
            .as_ref()
            .is_some_and(|a| a.symbol.starts_with("zil.store."))
    }) {
        return invalid(
            "E-CODEGEN-FOREIGN",
            "foreign symbol conflicts with owned Store runtime ABI",
            Some(f.span),
        );
    }
    let mut output = String::new();
    for (index, resource) in lir.resources.iter().enumerate() {
        let Type::OwnedStore { element } = &resource.payload else {
            continue;
        };
        let Type::Package { fields, .. } = element.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "Store runtime element is not a Package",
                None,
            );
        };
        let item = llvm_type(element)?;
        let align = alignment(element, &BTreeMap::new())?;
        let push = llvm_type(&Type::ErrorUnion {
            error: element.clone(),
            result: Box::new(Type::Unit),
        })?;
        let pop = llvm_type(&Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: element.clone(),
        })?;
        output.push_str(&format!(
            r#"
define internal i1 @zil.store.{index}.reserve(ptr %head, i64 %requested) {{
entry:
  %stride = ptrtoint ptr getelementptr ({item}, ptr null, i64 1) to i64
  %maximum = udiv i64 9223372036854775807, %stride
  %positive = icmp sge i64 %requested, 0
  %bounded = icmp ule i64 %requested, %maximum
  %valid = and i1 %positive, %bounded
  br i1 %valid, label %capacity, label %failed
capacity:
  %cap.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 2
  %cap = load i64, ptr %cap.field, align 8
  %fits = icmp ule i64 %requested, %cap
  br i1 %fits, label %done, label %prepare
prepare:
  %ptr.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 0
  %old = load ptr, ptr %ptr.field, align 8
  %size = mul i64 %requested, %stride
  %next = call ptr @zil.bytes.reallocate(ptr %old, i64 %size)
  %ok = icmp ne ptr %next, null
  br i1 %ok, label %publish, label %failed
publish:
  store ptr %next, ptr %ptr.field, align 8
  store i64 %requested, ptr %cap.field, align 8
  br label %done
done:
  ret i1 true
failed:
  ret i1 false
}}
define internal {push} @zil.store.{index}.push(ptr %head, {item} %item) {{
entry:
  %len.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 1
  %cap.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 2
  %len = load i64, ptr %len.field, align 8
  %cap = load i64, ptr %cap.field, align 8
  %room = icmp ult i64 %len, %cap
  br i1 %room, label %initialize, label %full
initialize:
  %ptr.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 0
  %data = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr {item}, ptr %data, i64 %len
  store {item} %item, ptr %slot, align {align}
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret {push} zeroinitializer
full:
  %tag = insertvalue {push} zeroinitializer, i1 true, 0
  %error = insertvalue {push} %tag, {item} %item, 1
  ret {push} %error
}}
define internal {pop} @zil.store.{index}.pop(ptr %head) {{
entry:
  %len.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 1
  %len = load i64, ptr %len.field, align 8
  %live = icmp ne i64 %len, 0
  br i1 %live, label %take, label %empty
take:
  %next.len = sub i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  %ptr.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 0
  %data = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr {item}, ptr %data, i64 %next.len
  %item = load {item}, ptr %slot, align {align}
  %result = insertvalue {pop} zeroinitializer, {item} %item, 2
  ret {pop} %result
empty:
  %error = insertvalue {pop} zeroinitializer, i1 true, 0
  ret {pop} %error
}}
define internal void @zil.store.{index}.drop(ptr %head) {{
entry:
  %len.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 1
  %ptr.field = getelementptr {{ ptr, i64, i64 }}, ptr %head, i32 0, i32 0
  %len = load i64, ptr %len.field, align 8
  %data = load ptr, ptr %ptr.field, align 8
  store i64 0, ptr %len.field, align 8
  br label %loop
loop:
  %index = phi i64 [0, %entry], [%next, %element]
  %remaining = icmp ult i64 %index, %len
  br i1 %remaining, label %element, label %release
element:
  %slot = getelementptr {item}, ptr %data, i64 %index
"#
        ));
        for (field_index, (_, ty)) in fields.iter().enumerate() {
            let Type::Managed(payload) = ty else { continue };
            let definition = lir
                .resources
                .iter()
                .find(|r| r.payload == **payload)
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        None,
                        "Store element field lacks its resource definition",
                    )]
                })?;
            let teardown = if definition.payload == Type::ByteBuffer {
                Some("zil.bytes.drop".into())
            } else {
                definition.teardown.map(symbol)
            };
            if let Some(teardown) = teardown {
                output.push_str(&format!("  %field.{field_index} = getelementptr {item}, ptr %slot, i32 0, i32 {field_index}\n  call void @{teardown}(ptr %field.{field_index})\n"));
            }
        }
        output.push_str("  %next = add i64 %index, 1\n  br label %loop\nrelease:\n  call void @zil.bytes.release(ptr %data)\n  ret void\n}\n");
    }
    Ok(output)
}

impl FunctionEmitter<'_> {
    pub(super) fn dynamic_store_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> CompileResult<()> {
        let output = result(instruction)?;
        if matches!(instruction.kind, InstructionKind::OwnedStoreNew { .. }) {
            self.line(format!(
                "  {} = freeze {{ ptr, i64, i64 }} zeroinitializer",
                value(output)
            ));
            return Ok(());
        }
        let store = match instruction.kind {
            InstructionKind::OwnedStoreReserve { store, .. }
            | InstructionKind::OwnedStorePush { store, .. }
            | InstructionKind::OwnedStorePop { store }
            | InstructionKind::OwnedStoreLen { store }
            | InstructionKind::OwnedStoreCapacity { store } => store,
            _ => unreachable!("Store instruction dispatch"),
        };
        let Type::Desc(payload) = self.value_type(store)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "Store operation requires a description",
                Some(instruction.span),
            );
        };
        match instruction.kind {
            InstructionKind::OwnedStoreReserve { capacity, .. } => {
                let callee = helper(payload, self.resources, "reserve")?;
                self.line(format!(
                    "  {} = call i1 @{callee}(ptr {}, i64 {})",
                    value(output),
                    value(store),
                    value(capacity)
                ));
            }
            InstructionKind::OwnedStorePush { value: item, .. } => {
                let callee = helper(payload, self.resources, "push")?;
                let ty = llvm_type(self.value_type(item)?)?;
                let result = llvm_type(self.value_type(output)?)?;
                self.line(format!(
                    "  {} = call {result} @{callee}(ptr {}, {ty} {})",
                    value(output),
                    value(store),
                    value(item)
                ));
            }
            InstructionKind::OwnedStorePop { .. } => {
                let callee = helper(payload, self.resources, "pop")?;
                let result = llvm_type(self.value_type(output)?)?;
                self.line(format!(
                    "  {} = call {result} @{callee}(ptr {})",
                    value(output),
                    value(store)
                ));
            }
            InstructionKind::OwnedStoreLen { .. } | InstructionKind::OwnedStoreCapacity { .. } => {
                let index = if matches!(instruction.kind, InstructionKind::OwnedStoreLen { .. }) {
                    1
                } else {
                    2
                };
                let pointer = self.temp();
                self.line(format!(
                    "  {pointer} = getelementptr {{ ptr, i64, i64 }}, ptr {}, i32 0, i32 {index}",
                    value(store)
                ));
                self.line(format!(
                    "  {} = load i64, ptr {pointer}, align 8",
                    value(output)
                ));
            }
            _ => unreachable!("checked Store instruction"),
        }
        Ok(())
    }
}
