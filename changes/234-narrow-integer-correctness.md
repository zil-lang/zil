# 234 LLVM窄整数常量正确性修复

施工11接续核查发现，main仍将负的i32等整数直接转换成完整u64后，以SignExtend=false交给LLVMConstInt。编译专项PR13的原报告及独立回归指出LLVM23.1.4可能保留不可见的高位，使IPSCCP在O2错误合并不同返回值。本片单独移植该明确正确性修复：在通过原machine preflight后，按目标1/8/16/32/64位取得既有模位宽整数位模式，再调用同一LLVM C API。保持合法输入、公开schema、ABI、预算、优化与双重Verify；不合入性能研究其余内容。

来源固定为research/incremental-compilation提交3d6babe17ee9d0a2f62d123063fb7af6a0886612的crates/zil-llvm-api/src/native/builder.rs、tests/builder210.rs和crates/zil-driver/tests/narrow_constants.rs，原说明为changes/research-narrow-integer-constants.md。既有研究误编译事实属于原身份，当前本地Clang17只能提供reference；本片正式LLVM23结果必须经新CI绑定精确候选取得，不重标旧结果。

回归保留所有位宽signed/unsigned同位模式及相邻不等控制，源级负返回分支和循环、byte_io NUL/高字节/长输入，API/reference、O0/O2和cold/warm对象缓存。新增源测试需保留编译报告、实际输入、stdout/stderr及退出码。新编译器对象缓存键绑定二进制身份，不能复用旧错误对象；本片不作性能比较。
