import impl_generators as derive
pub resource Tracked { pub code:i32 }
#emit derive.drop_impl(Tracked)
