//! Dense typed indices. Each index is only meaningful inside the table that
//! issued it; none of them is a stable cross-request identity.

macro_rules! index_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);
        impl $name {
            #[inline]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
            /// Panics only on a table with more than `u32::MAX` rows, which the
            /// request budget rejects long before construction.
            #[inline]
            pub fn from_index(index: usize) -> Self {
                Self(u32::try_from(index).expect("body IR table exceeds u32 rows"))
            }
        }
    };
}

index_type!(
    /// A storage slot of one body. Parameters occupy `0..param_count`.
    Local
);
index_type!(
    /// A basic block of one body.
    BlockId
);
index_type!(
    /// An interned type in a [`crate::body::TyTable`].
    TyId
);
index_type!(
    /// A nominal declaration in a [`crate::body::TyTable`].
    AdtId
);
index_type!(
    /// An interned generic argument list.
    ArgsId
);
index_type!(
    /// A variant of one nominal declaration, in declaration order.
    VariantIdx
);
index_type!(
    /// A field of one variant, in declaration order.
    FieldIdx
);
index_type!(
    /// A function of the module being compiled.
    FnId
);
index_type!(
    /// A closure definition; its captures are fields of its environment.
    ClosureId
);
index_type!(
    /// A dynamic interface declaration.
    DynId
);
index_type!(
    /// An entry of the body constant pool.
    ConstId
);
index_type!(
    /// A region variable of one body (solver inference variable).
    RegionVid
);
index_type!(
    /// A lexical cleanup scope of one body.
    ScopeId
);
index_type!(
    /// A foreign function declaration.
    ForeignId
);
index_type!(
    /// A static UTF-8 literal.
    StaticId
);
