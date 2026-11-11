# ThinLTO 离线两 case 质量探针

状态：脚本待非作者审查，尚未 prepare/import 这些脚本、未执行N64/N256编译/native。仅已按root授权执行独立两unit工具功能pilot，不运行其native产物；v1首败、v2成功均完整保存于pilot-2unit-v1/v2。此目录不是产品实现，也不证明编译提速。最初讨论的 O0 导出方案已放弃：本协议保留当前 O2 + incremental-o2 的前置折叠，消除该混杂。

## 源码与边界

基线 main990；研究源码 author e2e3ce1bc17cb16feafe38ebcf40c60229e2e922 与远端 eb5b150903b67d8b9011f6adc2d3ac67b1c419a6 同 tree 1da99d09dbeb53e59f8df4572f4bc5d010ff5929。前稿 2dc33f3 保留，仅静审发现无 feature 构建问题，未构建/运行；e2e 修正错误转换 cfg 和收据原子创建。

仅 build.rs 改动：环境变量 ZIL_RESEARCH_THINLTO_RECEIPT 指向新的绝对路径才开启；严格 Clang/O2/incremental-o2/after/Linux Main/8units/2jobs，无 closed/object cache。保留 O2 前端 fold、普通分区与 local/address 并组，禁用预复制 imports。原完整 machine、fold 后分区输入及实际每个最终 unit 全部 validate_module；当前独立 Verify 仍执行。外部 JSON 与 native_codegen.object_reports 同时记录检查事实，实际 LLVM 验证明确归外部 wrapper，不能混成 API producer receipt。未开启环境变量时，原调用路径保持原逻辑。

前端语义验证、完整 machine preflight、LLVM verifier 分三层：preflight 不证明语言内存安全，clang 成功不替代任何前两层。原型只使用继承的 pure 动态整数 fixture；此轮不声称已经覆盖函数地址和可变 global 正确性，扩大这些输入之前必须另测身份 controls。

## 实际链路与可审收据

A：冻结 main990 LLVM API full O2 + incremental-o2 + after，8 CGU/2 jobs，无 object cache。B：研究 exporter 同样 O2 前端，canonical owners + declarations → wrapper 实际 clang23 -O2 -flto=thin -march=x86-64 -fPIC → 实际 lld23 ThinLTO import/O2/after merge → ELF。

wrapper 保留实际 ZIL 发出的 argv、每 unit 原 IR 与 prelink bitcode，执行 opt verify-before、verify-prelink-after、llvm-bcanalyzer summary+HASH 检查、llvm-dis triple 检查。link 明确 pin --ld-path，启用 -pie、--lto-O2、--lto-CGO2、--plugin-opt=mcpu=x86-64、--thinlto-jobs=2、--threads=2、--icf=none、post-link merge-functions=true / aliases=false，保留默认 code model、不另设 target features。A 的实际 receipt 也检查 x86-64 / features empty / PIC / default code model。B 的 target profile 来自实际命令及bitcode，不是沿用 API receipt。clang 对已有IR执行-fPIC并不一定写PIC Level module flag；pilot-v1首败是草稿对此的错误断言，原失败及脚本保存。最终PIC由实际lld -pie保证：[createConfig设置Reloc::PIC_及默认CodeModel](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/LTO.cpp#L101-L116)；prelink flag有无如实记录，不据不存在的metadata冒充验证。

lld normal ThinLTO backend 接收 emit-index/import flags，调用实际 emitFiles，import 文件由真实 ImportList 对应的 source modules 产生。依据：[lld driver](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/lld/ELF/LTO.cpp#L185-L198)、[普通 backend 的 emitFiles](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTO.cpp#L1639-L1675)、[emitImportsFiles](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/LTO/LTO.cpp#L1575-L1586)。保存的 .imports 是真实来源模块列表；具体导入定义可从保存的 .3.import.bc 与输入模块核对，不能把“声明存在”当“主体已导入”。

link 成败均先复制 staging 的可用 LTO 临时文件；成功时要求8份 actual imports、8份 .4.opt.bc，并逐份执行 opt verify。缺格或工具版本/参数不兼容立即失败，保留 stdout/stderr/process-attempt/fatal 及已有临时文件，不补造收据。工具 pin 包括 clang/lld/opt/bcanalyzer/dis、Rust、两编译器的路径与完整 SHA，build-binding 同时校 source/tree/binary/build-log。每 wrapper 重校大工具哈希会增加请求 wall；此轮不据此 wall 作编译优化结论。

Thin native cache 每个 clean B compilation 使用独立新目录，前后列表和内容 hash 留存；当前质量 probe 不冒充 cache hit 实验。源码、cache、输出分离。所有子进程继承冻结两 CPU affinity，prelink最多2 workers、link2 ThinLTO jobs/2 linker threads，分阶段执行；RSS使用wait4 ru_maxrss，非进程树aggregate peak，可能有fork/launcher floor。工具identity保留绝对调用路径，不将末尾symlink解析成另一个argv0角色；字节SHA正常跟随symlink读取。lld必须传实际ld.lld路径，Rust传真实toolchain bin/rustc；不把通用lld/rustup驱动当同名工具。

## 冻结及执行

1. root 构建/绑定 A 与精确研究 B，提供 JSON binding（source_commit/source_tree/binary/binary_sha256/build_log/build_log_sha256/release_build_status=Pass）。
2. 非作者通过脚本审查后，运行 prepare.py；需要 --out、--baseline、--exporter、--rustc、--clang、--lld、--opt、--bcanalyzer、--dis、--baseline-build-receipt、--exporter-build-receipt，以及可选 --cpus 0,1。prepare只生成与原 frozen generator 一致的 n64/base、n256/base 和独立周期 oracle，不编译。
3. 再审生成 protocol 的精确 SHA；run.py --protocol ABS --protocol-sha256 SHA --out NEW_ABS --execute 才执行。无 --execute 不启动任何 compiler/native。已有输出目录拒绝覆盖。

vendor/ 五文件从 previous-evidence 的 bounded-imports/runtime-frozen-1679 原样复制，作为参考实现固定；调用其同一 generator、独立整数周期 oracle 与 stats，不执行旧 main。周期251、100000轮、模1000003；输入00/41/ff，leaf 8函数，所有 i32 中间值边界沿用原证明。两 case 为 k0，预生成源 hash 封存；不改变热循环、权重或叶函数规模。

进程计量改用独立processes.py（vendor不修改），仍计launch/input-through-wait4 wall、user+system CPU及ru_maxrss。外层每ZIL/native请求拥有独立session；wrapper实际工具不另建session，启动前验证getpgrp==getsid==getppid，确保直属私有ZIL session leader。内层deadline/异常先关闭输出并保存raw（未reap时CPU/RSS为null），再杀本次ZIL私有group；外层runner在组外wait4，且leader已reap后仍检查/杀残留，残留令请求失败。process-scope-pilot的normal、外层timeout、内层timeout三项仅用可控sleep子/孙进程验证，均无live残留；内层超时raw完整，外层SIGKILL时内部来不及写raw的事实保留，不伪造。该控制不含compiler/nativefixture采样。

每 case A/B/R各编译一次：6 clean compilations；每 arm两个额外功能输入、一次warmup、六formal：54 native总数，其中36formal。formal 使用原六排列 ABR/BRA/RAB/ARB/RBA/BAR，每arm各位置两次，非按结果重排；没有ABBA冒充三arm平衡。真实每一条 I/O 都核原独立oracle，无输出、异常、缺格均失败。所有六个formal算术均值，不删长尾；两 case分别要求 B/A≤1.10 且 R/B≥.90，均过才允许讨论原八 case的后续扩大，任一 NoGo 不继续矩阵。此结果不是功能全集验收或产品采用结论。

run 保存完整ZIL请求wall，包含额外检查/IR导出/clang prelink/thinlink/backend/native link；阶段子命令各有wall/CPU/RSS。但首轮仅runtime质量，不以cold诊断编译wall声称“净提速”，也不替代cold/warm/resident/多任务后续测试。异常raw先落盘，最后manifest不能留running。
