# CP11–CP13：LLVM23 第二轮

2026-10-30，第一轮精确候选 e8e274c1b28509547a4854d46708ae1601e756e4 已通过 2744 项默认配置测试。五语料、三个配对区块的新进程完整构建显示：O0 无编译缓存 ZIL/Rust 为 0.9581（描述区间跨 1）；O0 四请求增量为 1.4060；O2 ZIL 单 CGU 为 4.2448。Rust 为 1.99.1 原生 CGU 配置，另显式设置 overflow-checks=on、panic=abort、debuginfo=0、embed-bitcode=no。原始结果保留，不能将第一轮称为已超过 Rust。

## CP11：删除已确认的重复工作

- CP11a（/root/toolchain_setup）：一次请求内惰性共享 SourceMap 源文件索引。冷路径仍逐源扣费后检查重复 ID，只在全索引成功后保存；每个 map 仍重新检查节点限制、完整扣除 sources.len()、创建自己的 targets 并执行全部 entry/span/parent 检查。新辅助类型字段私有且绑定同一个不可变 source slice，不缓存验证结果或剩余额度。
- CP11b（/root/toolchain_setup，单独提交）：Normalize 的 Cow::Borrowed 分支移动原 combined.items，Owned 分支移动其 items；保留预算、文件定位、first-wins、Restore 和 imports 顺序。不得调用 into_owned 后声称消除了 Borrowed 克隆。
- CP11c（/root）：在机器 lowering 的调用与函数签名消费点直接移动已拥有的 Vec、Option<String> 和一次性类型，保留所有 fallible 调用及错误次序。完整机器模型及文本须保持相同。

写集：a 为 zil-base/src/source_map.rs、zil-expand/src/lib.rs 及对应测试；b 为 zil-expand/src/modules.rs 及对应测试；c 为 zil-backend-llvm/src/emit.rs、src/emit/resource_closures.rs 及限定证据。各自独立提交、非作者精确提交复核，再由协调者组合。

## CP12：显式可信本地前端检查点

/root/benchmark_audit 实施一个默认关闭的研究候选。现有 --cache-dir 仍是不可信原始源码候选；新增显式 --query-cache DIR（可信私有本地缓存）单独命名，不能把完整性哈希当作可信生产来源。首版只支持完整 build、相同全项目输入，不声称解决私有编辑的细粒度增量。

- 当前源码发现、路径歧义、解析/导入及 ArtifactPlan 检查仍先执行；完整源码/模块顺序/文件路径/导入、全部 11 个 Limits、schema/rules/target/选项和精确编译器生产身份必须绑定到键。
- 仅保存裸 TypedModule、CandidatePlan 及必需的原始项目元数据。不能保存或恢复 Verified 包装，不能恢复接口的 result_origins/callable_contracts/callable_returns/contract_fingerprint；它们由本次 Verify 后重新发布。
- 每次必须恰好独立 Verify 一次。键、头、完整性、长度、codec 不匹配在进入 Verify 前作为 miss；不能先 Verify 缓存失败再二次 Verify 且伪报次数。
- 保存并重新扣除原本需要累计的逻辑工作量，至少包括 CT 的逐模块 steps/nodes/函数费用；不同 Limits 一律 miss。不得提高语言限额以容纳缓存传输；codec 读写超限只能 miss/放弃写入，不能改变正常源程序接受集。
- 只在完整成功请求后原子发布缓存。错误、取消、半成品不能成为有效条目。显式缓存路径和可见 hit/miss 记录不得混入旧 disk_source 的计数。
- stop-after/emit-dir 或其它首版未覆盖入口走原管线并清楚记录 bypass。独立验证冷/未变/编辑、损坏/截断/身份漂移/限额 N 与 N−1/新请求/每次 Verify/旧产物保护等边界。

写集：zil-driver 中检查点模块、项目/源入口和最少必要公开选项，zil-cli 参数与 JSON，zil-ir 中私有原始项目元数据构造/codec 的最少必要支持，对应测试与文档。Wire27 外部格式、语言接受性及已核包装构造权限不变。若需要扩大写集或合同，先向协调者提供具体原因；常规签名借用适配可列出后实施。benchmark runner 的支持在候选接口确定后独立提交复核，重新冻结计划。

## CP13：O2 配置与优化范围

/root/llvm_strategy 先检查 O2 默认管线、函数可见性及多 CGU 支持，试验既有 8 CGU/4 jobs 配置。允许正常正确性/预检；正式计量由协调者统一静默窗口执行。任何优化管线或 linkage 改动必须先给出精确方案与语义/ABI边界，由协调者补充本计划后实施。不得因为单 CGU 慢就删除必需的优化或未核对运行退化。

## 验收与保留

所有候选须保留旧拒绝、预算、源位置和每次独立 Verify；不复用旧验证资格。CP11 完整模型/诊断差分，CP12 可信缓存边界及预算对照，CP13 完整构建与运行质量分别核对。正式测量前停止全部编译/测试；五语料和固定权重不变，O0/O2、无缓存/增量、CGU配置分别报告。初筛与确认、冷编译缓存与暖 OS 页缓存必须明确。任何失败、漂移、缺行禁止评分；有限研究不升级产品门。保留所有负结果，未证实候选不进入 main。

CP12 具体适配追加：授权 backend-llvm/src/lib.rs、object_cache.rs 仅导出/复用已有 trusted-installed-tool 只读身份 helper，不改原对象键、工具校验或 fallback。ELF build-id+元数据的可信安装前提必须明确，不能把它当作任意可替换生产者的字节证明；payload 仍单独校验。

### CP13 具体实现授权

显式 `closed_executable` 研究模式默认关闭，CLI 为 `--closed-executable`。只支持 Linux 的完整可执行 build、非 None 入口；emit/emit_with_options（包括外部 C harness）、原始机器 API、未启用的 build 及 Win64 原行为保持。显式模式不承诺对外导出生成的 ZIL 函数身份符号，但保留 main、外部声明、运行时 helper、全局对象和真实函数地址使用。

在原 partition 之后，根据本次 LIR 非 foreign FunctionId 生成精确候选白名单；扫描所有实际 CGU 的语义引用（包括函数地址及 global initializer），被其他单元引用的定义保持 External，其余有 body 的候选改 Internal。不依赖字符串前缀猜身份，不改变现有 partition 的 local-edge 共址。全套原机器预检和 default<O2> 管线保持。Clang 单 CGU 快路也必须实际消费同一变换后的 unit；不得只让 API 后端生效。原 sidecar 的 prepartition 性质和命中报告须明确；实际单元模型 hash 包含 linkage，旧对象不能误命中。

CP13 作者写集追加 backend-llvm/src/build.rs、machine_cgu.rs、对应测试/evidence/docs。CompileOptions 默认关闭字段、session 向 BuildOptions 的传递和 CLI 无值 flag/HELP，由 CP12 作者在其既有写集内作单独提交适配；两项提交保持独立审核。此字段只影响本次 Verify 后的 codegen，不是前端检查点可复用资格。运行质量须直接和未启用模式配对核对，不能仅凭正确输出通过便判定性能胜利。
