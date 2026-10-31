# CP29 native 证据与三套会话 harness 静态复核

审核 cp25_evidence。结论：**所列静态证据核查通过**。本次只读取作者保留的源码、IR、可执行文件字节、执行输出、构建收据及 Cargo 元数据；没有重新运行编译器、测试或 native 程序，不增加“非作者 native 重跑”计数。

## CP29 作者 native 控制

绑定实现 `a0f38d2bca8f01fc19ff1f2fa0a42c3983c223b8`，控制源码 SHA256 `5275c621600ba7a50a52b351a26f1f0a0a12a6a26deff6051f5c2c3abbda037f`。已逐文件核回收据列出的 **113 个文件，合计 2,048,744 字节**。

预期直接来自真实 ZIL 源码：main 求 `n=a.get()+b.get()`，b 固定返回 2；a 的私有 value 正文依次返回 1、5、1。main 再调用真实 C putchar 输出 `64+n`，最后返回 n。所以 cold/private/restore 的预期退出码为 **3、7、3**，stdout 为单字节 **C、G、C**（hex 43、47、43）。控制通过 `Command::output` 实际运行每个新产物；保存的六份退出码/stdout 与上述预期相符，stderr 均为空。

实际编译报告全部是 `phase=link`、`native_codegen.backend=clang`：使用 `clang -x ir -O0` 与 ld.lld，输入由 ZIL 编译阶段产生。产物头部为 ELF64 little-endian x86_64。此证据是实际 Linux 原生行为，**不能称为六次 LLVM API 后端执行**。该分野不影响它对 Types memo 的有限语义控制，但须准确写入最终结果。

三次 warm/fresh 对中，每对 Typed、Candidate、LIR、LLVM、verified-interfaces 五份原始表示均逐字节相同；native ELF、公开接口、诊断、输入身份与 source_files 也相同。cold 记录 reused=0/checked=6；private 与 restore 均为 reused=2/checked=4，保留三个纯 scalar 模块，含 foreign/unsafe 的 IO 模块继续实时检查。

private 之后插入一次真实签名错误：a.get 改为 bool，main 的加法随即在 Types 报 `E-TYPE-MISMATCH`。warm/fresh 完整诊断相同，Verify=0、artifacts 为空、未发布 must-not-publish。控制源码在这里断言旧 epoch 统计保持，随后 restore 的真实表示与 native 结果仍与 fresh 相同。本审查复核了该断言与保存结果，没有重新执行该断言，也没有将一次签名失败推广为所有原生构建失败模式。

详细机器收据见 `CP29-NATIVE-STATIC-ARTIFACTS.json`；可复核读取脚本为 `cp29_native_static_audit.py`。这里的 6 次 native 都属于作者执行证据，非作者新增执行数为 0，无性能评分。

## baseline / CP29 / CP30 构建绑定

三套 harness 绑定同一份 Rust 源：SHA256 `3aaf249ad90b67029bacedc66523ce149c230099af27fdc22ed2a6074ee5a37f`。根仓、CP29 构建快照、recovery 与原 v3 控制快照均为同字节。

基线实际构建 checkout 是 `6905ef3c2c3de99a9728824ee288c5ec8f5a266c`，与声明基线 `ee72fd6f701efa5f7a75ea3f13d4effc20d5c4b8` 的 crates tree 同为 `a7ae8da1862a69cf673b8db50431b4991cb0d1c5`。CP29 与 CP30 分别绑定其已审提交；CP30 顶层 driver rlib 与收据中的 deps rlib 为同一文件。

实际 Cargo fingerprint 显示三者一致：zil-driver 与 zil-backend-llvm 都是 `[default, llvm-api]`，zil-llvm-api 是 `[default, native]`，没有 timings 或额外 rustflags；同一 crate 的 profile/rustc/compile-kind 身份一致。链接均为 Rust 2024、-O、debuginfo=0、panic=abort、同一 LLVM 动态库目录与摘要。

只有 CP29 harness 添加 `--cfg cp29`，控制的唯一条件代码是编译调用计时结束后输出 memo 统计；baseline/CP30 输出 null。编译调用、请求序列、成功/Verify判断与观察量比较共享同源代码。全进程指标仍包含该统计报告成本，没有从候选成本中扣除。

正式 CP29/CP30 plan 中 A/B 二进制摘要分别与既有 baseline 冻结计划及新构建收据一致。由于正式窗口正在运行，本轮不批量重读大二进制/rlib；其现时摘要核验留待窗口结束的独立数据复算。baseline harness 的链接命令由作者追述，未找到独立的同时期链接命令文件；已有基线编译器构建收据、Cargo fingerprint，以及 v1/v2/v3 三个原始冻结计划支持其绑定。此局限明确保留，不补称当时已有完整链接日志。

详细绑定收据见 `SESSION-HARNESS-STATIC-BUILD-REVIEW.json`。静态一致不替代真实候选 control、完整计量或性能采用结论。

## 窗口结束后的补核

两窗结束后，完整独立数据复算已实际重哈希 CP29 的构建产物/工具、CP30 的全部 rlib/harness，以及 baseline 编译器和 Cargo fingerprint；均与原冻结收据相符。CP29 native 控制二进制本身的摘要也已闭合，仍没有由审核者重新执行 native 程序。

根随后使用原冻结 Rust harness 源路径、baseline 的 13 份 rlib 与相同 rustc flags，另行链接到新路径，返回码 0，链接 stdout/stderr 均为空。审核已重核 13 份 rlib 的摘要，并对原受测 baseline harness 与新产物逐块比较原始字节，完全相同，SHA256 均为 `24683943ee32f3fb4007450b0cf88d2acc7e7cfb3b6fdecf90493df7823d22cd`。这使源码/依赖/flags 与实际受测二进制的绑定得到字节级重现支持。原同时期独立链接日志缺失仍是历史事实；没有把后补重现改称当时记录。重现由 root 执行，本审核只核结果。
