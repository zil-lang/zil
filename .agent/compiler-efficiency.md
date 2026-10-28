# 编译效率研究

## 208
以177/Wire21+207和Ante-R1作为本次集成基线；LLVM API已进入CLI对象后端，但仍文本解析。原有权重和产品门不改。API verifier不能替代每请求独立Verify。FFI只在zil-llvm-api叶子crate，Clang默认对照保留。对象缓存、稳定身份、结构化CGU、直接Builder均未实现，不能把探针或文档记作能力完成。
