# 编译效率研究

## 208
以177/Wire21+207和Ante-R1作为本次集成基线；LLVM API已进入CLI对象后端，但仍文本解析。原有权重和产品门不改。API verifier不能替代每请求独立Verify。FFI只在zil-llvm-api叶子crate，Clang默认对照保留。对象缓存、稳定身份、结构化CGU、直接Builder均未实现，不能把探针或文档记作能力完成。

## 209主路线与同步
标准CLI现在默认启用LLVM API，原Clang为显式差分及无特性参考路径。此为用户授权的研发默认，不是完整成本与产品门Pass；直接Builder、稳定身份、结构化CGU和对象缓存继续沿同一生产接口推进，不再另造独立编译器。GitHub同步绑定远端实际SHA，不覆盖CI，不导入历史原始证据；必须push回读才算成功。
