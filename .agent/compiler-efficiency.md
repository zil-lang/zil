# 编译效率研究

## 208
以177/Wire21+207和Ante-R1作为本次集成基线；LLVM API已进入CLI对象后端，但仍文本解析。原有权重和产品门不改。API verifier不能替代每请求独立Verify。FFI只在zil-llvm-api叶子crate，Clang默认对照保留。对象缓存、稳定身份、结构化CGU、直接Builder均未实现，不能把探针或文档记作能力完成。

## 209主路线与同步
标准CLI现在默认启用LLVM API，原Clang为显式差分及无特性参考路径。此为用户授权的研发默认，不是完整成本与产品门Pass；直接Builder、稳定身份、结构化CGU和对象缓存继续沿同一生产接口推进，不再另造独立编译器。GitHub同步绑定远端实际SHA，不覆盖CI，不导入历史原始证据；必须push回读才算成功。

## 210（直接Builder）
当前LLVM API对象路径已改为结构化机器IR和LLVMBuilder；不再用文本解析API或文本CGU扫描。标准CLI中text-reference默认关闭；.ll诊断仍生成。
结构检查不替代独立Verify，不扩展语言接受集。对象缓存/稳定ID/ThinLTO仍待实施。旧209/206/207数值不拼接为210收益。
曾发现固定数组切片长度遗留字符串，已改为类型化Val；保留原失败回归并以完整工作区和独立209二进制对拍验证。
