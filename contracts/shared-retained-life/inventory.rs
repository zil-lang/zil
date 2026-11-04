/// Stable cross-request spelling of a REAL source/generated declaration. This
/// is identity data, never visibility, builtin, trait or borrowing authority.
/// The producer obtains it through SourceIndex/Origins, not parsed type names.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifeDeclarationIdentity {
    pub package: String,
    pub module_path: Vec<String>,
    pub declaration_path: Vec<String>,
    pub generation_path: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifeNominalKind { Record, Package, Resource, Enum }
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifeParameterKind { Type, Const(Type), Lifetime }
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifeNominalDeclaration {
    pub id: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    pub kind: LifeNominalKind,
    /// Full original declaration order, including phantom parameters.
    pub parameters: Vec<LifeParameterKind>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifeNominalActual {
    Type(Type),
    Const(Constant),
    /// The instance type's OWN qualified fixed scope, not an enclosing numeric
    /// class coincidentally having this ordinal. Outer maps explicitly compose.
    Lifetime(u32),
}
/// Complete semantic instance descriptor. Unique exact qualified Type mapping;
/// duplicate type -> different declaration/actual vectors must reject.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifeNominalInstance {
    pub declaration: LifeDeclarationId,
    pub ty: Type,
    pub arguments: Vec<LifeNominalActual>,
}
