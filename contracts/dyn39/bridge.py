from pathlib import Path
root=Path(__file__).parent
def edit(path, old, new, count=1):
 p=root/path;s=p.read_text();assert s.count(old)>=count,(path,old[:70]);p.write_text(s.replace(old,new,count))
ir='crates/zil-ir/src/lib.rs'; wire='crates/zil-ir/src/wire.rs'
edit(ir,'pub mod ct_wire;','pub mod ct_wire;\nmod dynamic;\nmod dynamic_fingerprint;\npub use dynamic::*;\nuse dynamic_fingerprint::*;')
edit(ir,'pub enum Type {','pub enum Type {\n    /// Unsized pointee. Address<Dyn> is the fat (environment,vtable) address.\n    /// Raw references require the complete declaration/interface context.\n    Dyn { interface: DynInterfaceId },')
edit(ir,'pub struct CoreModule {','pub struct CoreModule {\n    pub dyn_declarations: Vec<DynDeclaration>,\n    pub dyn_interfaces: Vec<DynInterface>,\n    pub dyn_vtables: Vec<CoreDynVtable>,')
edit(ir,'pub struct TypedModule {','pub struct TypedModule {\n    pub dyn_declarations: Vec<DynDeclaration>,\n    pub dyn_interfaces: Vec<DynInterface>,\n    pub dyn_vtables: Vec<DynVtable>,')
edit(ir,'pub enum CoreExprKind {','''pub enum CoreExprKind {
    /// Source is an actual borrowed concrete Self address, evaluated once.
    MakeDynBorrow {
        interface: DynInterfaceId, vtable: DynVtableId,
        source: Box<CoreExpr>, mutable: bool,
    },
    /// Callee is evaluated once before nonreceiver arguments and supplies arg0.
    CallDynSlot {
        callee: Box<CoreExpr>, interface: DynInterfaceId,
        slot: u32, arguments: Vec<CoreExpr>,
    },''')
edit(ir,'pub enum InstructionKind {','''pub enum InstructionKind {
    MakeDynBorrow {
        interface: DynInterfaceId, vtable: DynVtableId,
        source: ValueId, mutable: bool,
    },
    /// arguments excludes the receiver supplied by this exact callee.
    CallDynSlot {
        callee: ValueId, interface: DynInterfaceId,
        slot: u32, arguments: Vec<ValueId>,
    },''')
edit(ir,'ZIL-TYPED-MODULE-V38','ZIL-TYPED-MODULE-V39')
edit(ir,'ZIL-TYPE-IDENTITY-V35','ZIL-TYPE-IDENTITY-V39')
edit(ir,'    encode_len(bytes, module.functions.len());','''    encode_len(bytes, module.dyn_declarations.len());
    for row in &module.dyn_declarations { encode_dyn_declaration(bytes, row); }
    encode_len(bytes, module.dyn_interfaces.len());
    for row in &module.dyn_interfaces { encode_dyn_interface(bytes, row); }
    encode_len(bytes, module.dyn_vtables.len());
    for row in &module.dyn_vtables { encode_dyn_vtable(bytes, row); }
    encode_len(bytes, module.functions.len());''')
# The first match inside encode_type, not another matcher.
p=root/ir;s=p.read_text();start=s.index('fn encode_type(');idx=s.index('    match ',start);idx=s.index('{',idx)+1
s=s[:idx]+'''\n        Type::Dyn { interface } => { b.push(36); encode_dyn_interface_id(b, interface); },'''+s[idx:];p.write_text(s)
edit(ir,'fn encode_instruction(b: &mut impl FingerprintSink, i: &InstructionKind) {\n    match i {','''fn encode_instruction(b: &mut impl FingerprintSink, i: &InstructionKind) {
    match i {
        InstructionKind::MakeDynBorrow { interface, vtable, source, mutable } => {
            b.push(131); encode_dyn_interface_id(b, interface);
            encode_dyn_vtable_id(b, vtable); encode_u32(b, source.0);
            b.push(u8::from(*mutable));
        },
        InstructionKind::CallDynSlot { callee, interface, slot, arguments } => {
            b.push(132); encode_u32(b, callee.0); encode_dyn_interface_id(b, interface);
            encode_u32(b, *slot); encode_len(b, arguments.len());
            for argument in arguments { encode_u32(b, argument.0); }
        },''')
edit(wire,'pub const WIRE_VERSION: u32 = 38;','pub const WIRE_VERSION: u32 = 39;\n#[path = "dynamic_wire.rs"]\nmod dynamic;')
edit(wire,'            match ty {','''            match ty {
                Type::Dyn { interface } => { self.u8(36)?; self.dyn_interface_id(interface)?; },''')
edit(wire,'                26 => Type::U64,','                36 => Type::Dyn { interface: self.dyn_interface_id()? },\n                26 => Type::U64,')
edit(wire,'        self.count(module.functions.len(), self.limits.max_functions)?;','''        self.count(module.dyn_declarations.len(), self.limits.max_nodes)?;
        for row in &module.dyn_declarations { self.dyn_declaration(row)?; }
        self.count(module.dyn_interfaces.len(), self.limits.max_nodes)?;
        for row in &module.dyn_interfaces { self.dyn_interface(row)?; }
        self.count(module.dyn_vtables.len(), self.limits.max_nodes)?;
        for row in &module.dyn_vtables { self.dyn_vtable(row)?; }
        self.count(module.functions.len(), self.limits.max_functions)?;''')
edit(wire,'        let count = self.count(self.limits.max_functions, 4)?;','''        let count = self.count(self.limits.max_nodes, 1)?;
        let mut dyn_declarations = Vec::with_capacity(count);
        for _ in 0..count { dyn_declarations.push(self.dyn_declaration()?); }
        let count = self.count(self.limits.max_nodes, 1)?;
        let mut dyn_interfaces = Vec::with_capacity(count);
        for _ in 0..count { dyn_interfaces.push(self.dyn_interface()?); }
        let count = self.count(self.limits.max_nodes, 1)?;
        let mut dyn_vtables = Vec::with_capacity(count);
        for _ in 0..count { dyn_vtables.push(self.dyn_vtable()?); }
        let count = self.count(self.limits.max_functions, 4)?;''')
edit(wire,'        Ok(TypedModule {','        Ok(TypedModule {\n            dyn_declarations, dyn_interfaces, dyn_vtables,')
edit(wire,'            InstructionKind::LoadStoredBorrow {','''            InstructionKind::MakeDynBorrow { interface, vtable, source, mutable } => {
                self.u8(131)?; self.dyn_interface_id(interface)?;
                self.dyn_vtable_id(vtable)?; self.u32(source.0)?; self.boolean(*mutable)?;
            },
            InstructionKind::CallDynSlot { callee, interface, slot, arguments } => {
                self.u8(132)?; self.u32(callee.0)?; self.dyn_interface_id(interface)?;
                self.u32(*slot)?; self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments { self.u32(argument.0)?; }
            },
            InstructionKind::LoadStoredBorrow {''')
edit(wire,'            130 => InstructionKind::StoreStoredBorrow {','''            131 => InstructionKind::MakeDynBorrow {
                interface: self.dyn_interface_id()?, vtable: self.dyn_vtable_id()?,
                source: ValueId(self.u32()?), mutable: self.boolean()?,
            },
            132 => {
                let callee = ValueId(self.u32()?);
                let interface = self.dyn_interface_id()?;
                let slot = self.u32()?;
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count { arguments.push(ValueId(self.u32()?)); }
                InstructionKind::CallDynSlot { callee, interface, slot, arguments }
            },
            130 => InstructionKind::StoreStoredBorrow {''')
# The shared publisher owns all final common model codecs/version bumps.
base='crates/zil-base/src/lib.rs';edit(base,'pub const SCHEMA_VERSION: u32 = 38;','pub const SCHEMA_VERSION: u32 = 39;')

ast='crates/zil-syntax/src/ast.rs'
edit(ast,'    Dyn {\n        path: Vec<String>,\n        args: Vec<GenericArg>,\n    },','''    Dyn {
        path: Vec<String>,
        args: Vec<GenericArg>,
        /// Explicit closed binding/finite slot list; never inferred from uses.
        bindings: Vec<DynBinding>,
    },''')
edit(ast,'#[derive(Clone, Debug, PartialEq)]\npub enum GenericArg {','''#[derive(Clone, Debug, PartialEq)]
pub struct DynBinding { pub kind: DynBindingKind, pub span: Span }
#[derive(Clone, Debug, PartialEq)]
pub enum DynBindingKind {
    Type { member: Vec<String>, value: TypeRef },
    Constant { member: Vec<String>, ty: TypeRef, value: Expr },
    Method { member: Vec<String>, arguments: Vec<GenericArg> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenericArg {''')
edit('crates/zil-syntax/src/parser/types.rs','TypeKind::Dyn { path, args }','TypeKind::Dyn { path, args, bindings: Vec::new() }')
edit('crates/zil-syntax/src/parser/items.rs','            Some(self.ty()?)','''            let mut value = self.ty()?;
            // Only a direct dyn alias RHS accepts this block. A function body
            // after `-> &dyn Trait` is never consumed as a type binding block.
            if matches!(value.kind, TypeKind::Dyn { .. }) && self.at(Symbol::LBrace) {
                let bindings = self.dyn_bindings()?;
                let TypeKind::Dyn { bindings: target, .. } = &mut value.kind else { unreachable!() };
                *target = bindings;
            }
            Some(value)''')
edit('crates/zil-syntax/src/parser/items.rs','    fn const_decl(&mut self, associated: bool) -> P<ConstDecl> {','''    fn dyn_bindings(&mut self) -> P<Vec<DynBinding>> {
        self.expect(Symbol::LBrace)?;
        self.scoped(true, |p| {
            let mut bindings = Vec::new();
            p.separators();
            while !p.at(Symbol::RBrace) && !p.eof() {
                let start = p.here();
                let kind = if p.eat_word("type") {
                    let member = p.qualified_kind(QuoteNameNamespace::Type)?;
                    p.expect(Symbol::Eq)?;
                    DynBindingKind::Type { member, value: p.ty()? }
                } else if p.eat_word("const") {
                    let member = p.qualified_kind(QuoteNameNamespace::Value)?;
                    p.expect(Symbol::Colon)?;
                    let ty = p.ty()?;
                    p.expect(Symbol::Eq)?;
                    DynBindingKind::Constant { member, ty, value: p.expr(true)? }
                } else if p.eat_word("fn") {
                    let member = p.qualified_kind(QuoteNameNamespace::Value)?;
                    p.expect(Symbol::ColonColon)?;
                    let arguments = p.type_args()?;
                    DynBindingKind::Method { member, arguments }
                } else {
                    return p.fail("E-PARSE-DYN-BINDING", "dyn bindings contain type/const equations or explicit method applications");
                };
                let span = p.finish(start);
                p.end()?;
                bindings.push(DynBinding { kind, span });
                p.separators();
            }
            p.expect(Symbol::RBrace)?;
            Ok(bindings)
        })
    }
    fn const_decl(&mut self, associated: bool) -> P<ConstDecl> {''')
