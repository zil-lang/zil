# 增量编译纵向专项

状态：Doing；长期研究分支：`research/incremental-compilation`。

用户于本会话授权建立新的专项、持续攻坚编译速度并直接使用 GitHub 权限发布。正式父目标为 B0/W4；本记录不变更父工单状态或产品门。

- 基线：`21058d0e331de764e3a1b9123c3e1edf5e55a1aa`（开工已回读 GitHub main）。
- spec_hash：`b94813aee9a57ce9810a5fe59e3ed7a060c3fead985c5cd6632c9deadf4abdcb`。
- normative_hash：`5e0b190fff701bcaef8ffb1cbd818261584ee2795f69dad514668e497719f3c1`。
- 当前权威记录：本分支本文件；复用已有隔离 checkout，不增加并行写入者。
- 首片写集：`crates/zil-backend-llvm/src/machine_cgu.rs`、`crates/zil-driver/tests/object_cache212.rs`、本记录、`docs/research/incremental-compilation/**`、`tools/research/incremental-compilation/**`、`evidence/research/incremental-compilation/**`。
- 公共 schema、Cargo.lock、默认逻辑预算、产品门和语言接受性不在首片写集。
- 首片产物：精确声明依赖的 CGU 分区、真实源码对象缓存反例及必要反向控制、限定测试、独立审核与精确提交收据。
- 限定命令：`cargo test --offline --locked -p zil-backend-llvm --features llvm-api --lib`；`cargo test --offline --locked -p zil-driver --features llvm-api --test object_cache212 --test impl083_bytes_native --test impl120_field_backing_native`。
- 阶段状态：施工。形成可编译检查点即提交推送；独立审核绑定实现提交，未经审核不宣称 main 集成或性能突破。

完整路线、旧图切割/zit 的适用边界及四目标见 `docs/research/incremental-compilation/PLAN.md`。后续片开始前在此明确追加写集与控制，不把待做路线记为已实现。

## 往来

## 第二阶段：持续推进组合实现与完整计量

用户明确要求本轮持续推进至候选收敛与端到端验证，不以首个补丁完成作为停点。基线为 `5d7c1eeb2b74052eb239e19529dc5a943c6622a3`；规范身份保持上述值。

追加写集：`crates/zil-db/src/**`、`crates/zil-driver/src/session.rs`、`crates/zil-driver/src/project.rs`、`crates/zil-driver/src/lib.rs`、`crates/zil-driver/tests/**`、`crates/zil-ir/src/lib.rs`、`crates/zil-ir/tests/**`、`crates/zil-verify/src/lib.rs`、`crates/zil-syntax/src/**`、`crates/zil-syntax/tests/**`、`crates/zil-llvm-api/src/preflight.rs`、`crates/zil-lower/src/lib.rs`、`crates/zil-backend-llvm/src/**`、`crates/zil-expand/src/lib.rs`、`crates/zil-expand/src/ct_selection.rs`、`crates/zil-expand/src/receivers.rs`，以及本专项工具、文档和证据。不得改变语言接受集、逻辑额度、原始指纹编码、公共Wire或产品阈值。

先组合CP34流式编码与CP35共享核验输入的可审计部分，修复Session全清策略，继续减少热路径重复求值。每候选记录隔离效果、失败、资源边界与fresh对照，最终在完整CLI与连续Session中分别计量；性能没有证据的候选不直接算收益。

- 2026-10-31：从远端 main 建立专项；首片优先消除未用外部声明对所有 CGU 的失效放大。当前 Session 容量断崖、整项目语义重做及核验/Lower 重复成本已定位，后续依纵向路线继续。
- 2026-10-31：首片实现 `665a0a06dc36a94f19e9ee8bbcd87fab321c6423` 已推送；限定64测试通过，后端Clippy通过。按独审意见收紧错误拒绝断言，最终功能审核绑定 `8fb69d61b41a241806e7021de93a27ce7e213f7b` 为Pass。200次完整CLI screen和200次native均通过；收益限于声明失效控制，保留private编辑约1.1%不利点估计，未宣称产品突破。长期专项继续Doing，main未集成。


## 混合源码验证发现的后端正确性修复

第二窗口后在真实 byte_io 示例发现原基线已有的 LLVM API O2 常量传播错误。按 `changes/research-narrow-integer-constants.md` 先记录语义修复，再追加写集：`crates/zil-llvm-api/src/native/builder.rs`、`crates/zil-llvm-api/tests/builder210.rs`、`crates/zil-driver/tests/narrow_constants.rs`、该changes提案。此修复不降优化级别或减少独立验证；错误基线O2不计等价速度收益。

## 本轮组合交付

- 最终生产实现 `5cf1c417f5cbdc0895e5edceb3053d9d4da490c6`，b230组合实现、646过时owners释放与5cf常量修复均经精确提交独审Pass；最终486项限定测试及受影响库Clippy通过。
- 四个完整窗口各1560请求，分别保留首轮RSS回退、owners修正、最终正确性修正及开工main直接对照；不叠乘历史收益。另320次真实混合源码编译及16次文本参考控制，旧O2错误组不计算等价速度比。
- 从main直接对照512模块暖Session check 105.515→38.183 ms，128模块暖build 57.034→43.331 ms；512模块私有修改check 108.016→104.204 ms。冷CLI与小语料有不利点，完整表列。报告、外部研究和可校验原始证据进入跟踪路径，见 `docs/research/incremental-compilation/RESULTS.md`。
- 工单保持Doing：函数级语义增量、完整链接动作模型与分段核验尚未完成；不改父任务/产品门，不自行合入main。云安装/启动配置已保存，当前固定Rust1.99.1与LLVM23.1.4。

## 第三阶段：完整当前输入的前端复用

阶段2发布后继续沿实际phase计数推进，发现暖项目仍补做已淘汰的Parse。先记录`changes/research-project-frontend-reuse.md`；在已有session/project与driver tests写集内实施精确当前Typed原始结果的复用，保持独立Verify与所有产物/预算/取消边界。另追加上述提案及本专项文档/工具/证据；旧最终报告和性能窗口保留原身份，新结果单独冻结，不把它称为函数级语义增量。

## 第四阶段：产物路径重复解析

阶段3已完成两套各1560请求窗口，继续针对暴露出的共同父目录重复解析成本。先记录`changes/research-artifact-directory-reuse.md`，追加写集`crates/zil-driver/src/artifacts.rs`及该提案。只复用一次检查批次的一个成功parent解析，逐文件当前物理身份、错误和每次发布检查不省略；限定测试与独审后重新完整计量。不得跨请求缓存路径安全结论。

## 第三、四阶段完整交付

- 当前生产9fe5f55e；a4前端复用、5e真实源码补强、9fe当前批次路径解析均独审Pass。最终495限定测试、all-targets Clippy与fmt通过。
- 新四窗共9360请求、8160当前ZIL Verify、3720 native控制；另混合源码320构建/native及16参考构建/native。最终两窗各10区组3120请求，保留所有不利单元及旧O2错例。
- 最终main直接对照512模块暖check 106.917→13.443 ms、私有修改107.113→94.652 ms，128模块暖build 55.531→35.479 ms。当前完整报告为`docs/research/incremental-compilation/CURRENT.md`；原RESULTS/MEASUREMENTS保持阶段2身份。
- 保持Doing及集成边界；函数级语义增量、完整链接动作和分段证书仍未完成。最终GitHub自动检查绑定PR精确HEAD，不用前一提交240bb37d8已通过的run替代新源码验证。

## 第五阶段：分布扩展、Rust同工作负载与算法归因

用户明确要求补足大模块/模块规模分布、核实旧图切割与zit效果，并对同一基准中的Rust差距继续优化。基线为12fc97bde（生产同9fe）。新增研究工具/文档/证据仍在既有写集；先冻结形状、实际源尺寸、请求序列与等价原生控制，区分CLI/Session、单crate/多crate、检查/构建、O0/O2，保留失败/限额/不利分布。不以原简单标量树代表大模块或真实复杂程序，不改变默认预算/正式产品门。

性能诊断确认生产热点后另追加具体生产写集及必要提案；最终独立审核绑定精确实现和证据。历史档案保持不变，旧算法研究成绩不重标为本轮收益。

第五阶段生产候选前置提案为changes/research-large-module-codegen.md；追加build.rs内部调用与该提案写集。先仅O0显式多CGU固定分块，O2保留原分区/优化质量；不可把O2差距称为已修复。真实源码对象复用及独立审核后计量。

第五阶段第二候选先记录changes/research-closed-executable-port.md，移植旧CP13显式封闭构建；追加CLI main.rs及CLI限定测试写集。保持默认false及公共Wire/JSON形状，默认/closed分别计量和独审，来源绑定旧e7b492c6。

第五阶段第三候选先记录changes/research-dominator-intervals.md，在既有preflight.rs写集内为当前支配树构造大图区间索引，独立旧walk差分后测完整成本，保持SSA/phi/不可达语义与首错。

第五阶段冻结801五窗的分项继续暴露大模块命中后的机器预检成本。先记录changes/research-preflight-fanin.md；在既有preflight.rs写集内评估高扇入idom合并的当前树路径投影，以及只用于函数内查找的容器。必须待原独占测量完成再构建/测试，新候选另列身份和窗口，完整保留原801数据与不利结果。

## 第五阶段完整交付

- 最终生产为59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9：O0大文件分块、显式封闭可执行文件、支配树区间查询、高密度扇入路径投影及函数内查找均完成实现、反向控制和独审。05e中间候选的准备费门槛已收紧，不能冒充最终生产身份。默认O2优化级别、独立Verify、逻辑额度和公共Wire保持原合同。
- 23生成族加14真实示例族，补充单文件128—8192函数、均匀/偏斜模块、长函数、分支、泛型、CT、类型、资源及闭包。801五窗7824请求/652单元，59d三窗2700请求/225单元，合计10524请求/877单元；两族原默认预算拒绝造成40个无效单元，保留失败，不调高预算。最大成功单文件456774字节/16388行。样本仍不代表大型工业多crate程序。
- 最终113限定测试、all-targets Clippy及fmt通过；独立43 API/文本参考测试、5087 CFG、5975139块对以及26320次完整预检差分通过。最终23生成族与14真实族另作204请求组合控制及14 oracle构建，其计时不混入正式窗口。数据复算及文档独审均Pass，精确交付与GitHub CI随后绑定最终HEAD。
- Rust同负载最终8192函数：默认O0冷构建比0.660、早期私有编辑0.833、后期私有编辑0.626；默认O2早期编辑仍4.072倍，显式closed为1.500倍。最终预检隔离窗口O2无改动完整请求1025.708→910.377 ms，预检232.243→115.835 ms；同时保留512函数/O2无改动约4.1%回退及所有不利单元，不叠乘不同窗口历史收益。
- 旧076图切割的局部收益不稳定，080依赖传播及旧CP13封闭构建有可复用部分；zit只验证其稳定依赖/相等屏障等机制，并复现无环图误报cycle的反例，不宣称当前编译器已集成zit或取得其性能收益。完整归因、Rust差距和未完成路线见docs/research/incremental-compilation/ALGORITHM-ATTRIBUTION.md。
- 最新结论入口为docs/research/incremental-compilation/README.md与DISTRIBUTION-RESULTS.md；两个完整表列出全部877单元，证据在stage5与stage5-followup。既有CURRENT/RESULTS及历史证据保持原身份。工单继续Doing，持久化函数级语义增量、完整链接动作模型仍未完成；不自行合入main或改变父任务、产品门。

## 第六阶段：重新验证图切割、适配zit并推进O2/语义增量

用户明确要求不得沿用旧分布结论停止研究，须修复适配并实施可用思路，继续追赶Rust O2与增量。基线3d6babe17ee9d0a2f62d123063fb7af6a0886612；规范hash保持既有值。复用当前专项分支/主工作树，按互斥写集并行子任务，root统一提交，作者与独审分离。

追加研究写集tools/research/incremental-compilation/stage6_algorithms/**、docs/research/incremental-compilation/STAGE6*、evidence/research/incremental-compilation/stage6/**及各research提案。生产候选分配：O2稳定分区/导入由o2_graph_implementation持有backend machine_cgu/build与llvm-api必要内部表示；zit_incremental_implementation持有zil-db与driver/session；semantic_incremental持有zil-analyze并与session作者协调接入；root持有driver/lib/project、CLI及相应测试。公共Wire/schema、Cargo.lock、默认额度、Verify独立性与产品门保持；新增策略先显式声明，不靠降优化级别制造速度收益。旧076资源联合关系和CGU图严格分清，修复zit适配不重标原JS场景的设计为通用bug。

每个候选先提案与真实正反例，限定测试与独审绑定源码；最终资源独占交错计量，记录完整请求、运行行为/质量、RSS、缓存失效及不利结果。handoff历史完整性检查仍因精简恢复历史/旧记录缺失为Fail，不重标全库历史验收状态；已审核现行生产与专项Doing记录继续授权当前研究。

第六阶段初筛后追加：与实际Rust1.99.1源码/工作产品比对确认ThinLocal和持久语义查询是具体差距，继续制作实际CGU ThinLTO隔离对照及显式可信语义候选缓存。ThinLTO独立工具写集tools/research/incremental-compilation/thin_local/**及提案，当前EE生产不改；semantic_incremental继续持有driver/project、lib、artifacts、新semantic_cache及相应测试/session协调接入，root负责CLI。新增trusted缓存为显式选项，既有source缓存不改，完整源/规则/limits/target/compiler身份绑定，每次当前Verify保持；不以内容hash证明不可信来源。先控制/独审，最终另固定实现与性能身份；EE初筛仍只测保留的EE二进制。

Rust精确源码又确认O2默认MergeFunctions与当前LLVM默认管线的具体差异。追加backend/API MergeFunctions可选择候选（off/after/before-and-after），由backend作者持有pass/缓存key与控制，root接driver/CLI；先保持默认off，函数地址/导出/FFI/资源语义不改，Clang无法支持的early模式显式Unsupported。不得把原ThinLTO初筛或原529语义缓存性能重绑到新生产；新release与消融另冻结。

精确Rust比对再确认O2与O3的SLP开关不同。按changes/research-o2-slp-ablation.md在隔离43037快照仅关闭SLP并更新真实pipeline/cache身份，追加tools/research/incremental-compilation/slp_ablation/**写集。当前生产保持原行为；独立构建/控制和同负载编译及运行质量消融后再决定，不将Merge窗口收益归给此候选。

真实机器IR审计发现只读参数和单源块标量仍产生大量不可观察的栈往返。按changes/research-scalar-local-promotion.md追加emit.rs、emit/scalar_locals.rs及driver scalar_local_promotion控制写集，由llvm_shape_audit转为该候选作者；保留独立review_incremental_slice1审核分离。仅允许白名单、无地址暴露且当前定义可证明的标量；不重排调用/trap、不引入跨源块Phi或更改预算/Verify。先作为待审候选实现，最终必须同Merge/SLP设置消融后决定保留，不以静态IR计数宣称收益。

DB六区组独立消融完成，未证明净提速；真实driver的内容版本键让反向索引不能改善主要编辑路径。按changes/research-reactive-key-attribution.md将默认zil-db/src/lib.rs完整恢复3d6原字节，保留EE实现、修复后的JS适配、差分控制和全部正负实验身份。Analyze init投影、语义候选缓存、分区和scalar promotion分别保留独立候选身份；不把组合O0收益归于撤回的DB。稳定查询身份/有效性/依赖与等值屏障不是删除input哈希的小修，不在此次撤回中暗改。

实际Session事件与源码审计进一步定位私有编辑997/1025重解析：32 raw owner中整项目四阶段占四项，其余仅28 Parse；旧ExpandedProject未持有项目AST。按changes/research-project-syntax-retention.md在既有driver project/session与测试写集内，由semantic_incremental实施当前项目原始Parse Arc子所有权及精确身份/费用复用。必须同时接入filesystem loader真实计算路径，不能只改变事件计数；借旧语法索引时仍释放旧Core/Typed，保留当前输入检查、限额、产物、错误及取消边界。新增常驻AST字节需实测RSS，不把owner数量上界冒充字节上界，也不声称实现函数级Types增量。

实际LLVM自身进程采样将大函数O2热点定位到JumpThreading入口块替换反复全量重算支配树。按changes/research-entry-preserving-jump-threading.md追加tools/research/incremental-compilation/jump_threading_plugin/**及pass_timing/**独立研究工具写集；固定上游23.1.4源码/SDK，原版clone与内建pass先对照，再保持入口块与WeakTrackingVH工作表。只在隔离opt插件/私有捕获快照验证，不修改生产LLVM API或默许新的C++边界；原Verify、机器预检、当前工具/管线缓存身份、故障记录和成功请求发布均须保留。限定独审与正式完整请求收益分别绑定，不能把pass时间下降冒充端到端或Rust胜出。

JumpThreading插件限定正确性控制通过后，继续评估单对象工具链落地：在隔离官方23.1.4 PIC archives副本中，仅替换保持原类布局的JumpThreading.cpp.o并重链私有libLLVM；由独立prefix与显式研究pipeline/cache身份区分，实际加载库必须核对。旧SDK、LLVM安装、已冻二进制与缓存不得覆盖。此候选仍属于研究工具链，先限定正确性/独审及实际API完整请求验证；不把opt包装器时间直接作为生产后端结论。

上游主分支已推进f2572502beb76eb86e9254dea855404e4998a64d，专项PR发生Expand接口冲突。按主线已批准的232/233/234语义导入主分支到专项，唯一手工解决同时保留共享声明索引和跨模块trait所需完整模块/imports。旧阶段规范hash和已冻二进制证据不改写；合并后记录新规范身份并做限定组合控制与新HEAD远端CI，不自行合入main。

CI闭环追加写集：.github/workflows/verify-main.yml（以实际现有主验证workflow文件名为准）、tools/ci/research_disk_space.py及其限定测试、changes/research-ci-disk-recovery.md。41206406105先发现DB撤回时tests.rs未同步，888af53逐字恢复配套3d6测试（13唯一/26执行）且release ELF逐字不变；随后41207249551被runner ENOSPC硬终止，未取得全量测试结论。仅在GitHub托管Linux临时运行器准备阶段清理固定白名单的未使用预装工具链，先记录容量/路径并立刻上传收据；不得删源码/测试/原始验证证据，不改测试集合/命令、预算或产品门。原两次失败保留，新完整CI另绑定。

研究LLVM组合需要完整远端验证，追加独立写集.github/workflows/research-llvm.yml、tools/ci/research_llvm_manifest.py及限定控制、changes/research-llvm-ci.md。保持官方verify-main.yml不变；研究workflow仅专项push和显式dispatch，复用官方全量workspace/reference/native/Clippy/证据保存及成功归档后缓存释放逻辑。固定SDK一次下载、校验后同时供官方安装和精确headers，按已冻四TU组合工具分别重链original/candidate，测试只显式加载candidate/lib并核对实际check_toolchain库SHA与来源收据。完整源码/patch/log/receipt失败也归档，不上传SDK、动态库或target。研究时限可独立90分钟，不降低测试/预算，也不把远端正确性验收称作本地性能结果。root独审、统一提交，不自行合入main。

2026-11-01：继续在实际 73d6 差距上筛选完整当前输入的 O2 工作削减，边界见 `changes/research-current-input-o2-work.md`。追加明确写集 `crates/zil-llvm-api/src/lib.rs`、既有 `preflight.rs`、driver/backend 内的私有摘要、来源身份、图和除数模块，以及对应限定控制与新版本测量工具。常量摘要和验证专用路径已完成各自隔离比较，图必要条件剪枝、稳定身份/闭合删除、常量除数仍分别筛选；最终由 root 一次组合并以新源码身份进行 Rust 完整确认。作者、独审、诊断和正式时间窗口保持分离，原失败及所有负结果保留；Doing 和产品门状态不变。

## Stage 7: restored environment and focused Rust-gap work

2026-11-01: user requests continued implementation in this conversation and fewer redundant experiments. Restored the published older environment, fetched specialty HEAD 66f15461, rebuilt the selected four-TU LLVM candidate and current compiler. Old local Stage 6 raw records and session98995 completion receipt are absent; no completion or final archive is inferred. Main remains untouched. Use four representative cases for diagnosis and screen, then one meaningful confirmation after selecting an implementation.

Current scoped candidate is `changes/research-verified-scalar-summaries.md`: bounded scalar summaries after current Verify/bind_lir and before machine CFG construction. Existing backend/research writing scope applies. Current handoff's historical integrity Fail is not upgraded; full-language/product acceptance is unchanged. Independent reviewer is review_incremental_slice1; root implements.

Stage 7 implementation frozen at `0691a186e5066b62a53eca4bd706da87ba65a3a3`: verified scalar summaries before machine lowering, enabled only by existing incremental O2. Four focused real-source controls (22 native builds), backend all-targets Clippy and fmt passed; independent code review Pass. Three-repeat large-module confirmation: 135 valid requests/native results, plus six valid opaque guard requests. Paired geometric means candidate/baseline: default warm .766946/edit .853648; closed warm .756411/edit .843123. Even-module edit/Rust remains 2.070422 default and 1.360076 closed; two individual cold regressions retained. Opaque edited executables byte-identical; single repeat gives no broad performance conclusion. Exact sources, logs, failures, all cells, restored environment limitations and independent statistical review are consolidated in STAGE7-RESULTS.md and evidence/.../stage7. No product threshold, public schema, Cargo.lock or main change; task remains Doing, persistent semantic incrementality unfinished.

## Stage 8: Rust-informed frontend preparation reuse

User requests continued implementation with Rust references. Baseline1498bfee9; fixed Rust1.99.1 code audit confirms eval_always frontend preparation plus owner/signature/typeck projections, not whole-AST caching. Proceed under changes/research-frontend-preparation-reuse.md. Add explicit zil-types/src/{inference,program,lib,preparation}.rs and its focused tests to writing scope; existing Expand/driver/research scopes apply. stage8_frontend_audit implements only modules.rs owned redistribution; root implements current-input inference/preparation reuse; independent review is separate. Preserve raw API, exact budget/first errors, current Verify, and Doing/product states. Stage7 remote official/research CI still running at start; do not rerun its benchmark matrix.

Stage8 f289037fe passed 18 focused checks and independent review, but its 135-request confirmation showed only small/inconsistent complete-request gains. Keep the measurements without calling it a breakthrough. Extend the same candidate with explicit zil-ct/src/source.rs/lib.rs scope: bounded plain-input classification and allocation-free current CT validation, reusing the immutable current Core owner only after the full original charges/checks. stage8_frontend_audit authors CT validator; root authors Arc ownership/driver integration and differential controls. Unsupported shapes retain original evaluation. Initial first-screen overlapped final build and is explicitly excluded; isolated-screen/confirmation remain separate valid windows.

Stage8 final implementation 8c5dcc072318f9c496bdc19e14cbbbaadccccc2e: 24 scoped tests, four-crate all-targets Clippy and independent code/statistical review Pass. Final 75 builds/native oracles valid: target uniform8x1024 three histories, single8192/skew8 one each. Target edit paired ratios default0.945173/closed0.898440; current Rust ratios1.980363/1.173982. Current-process maximum RSS drops about15%, ten final edited executable pairs byte-identical; four opaque frontend checks/current Verify pass, without repeating the opaque LLVM benchmark. Explicit adverse findings: target default warm1.083521 (all three pairs regress, semantic/object hits), skew default one-pair cold/edit regress; cause unresolved, no general net-speedup claim. Frozen first/middle/final windows, failures, commands, resource data and Rust source references in STAGE8-RESULTS.md and evidence/.../stage8. Old1498 official/research CI both success; newHEAD must be read separately. No main/environment publishing or product-gate change. Continue Doing; next diagnosis is warm-hit cost, then stable declaration/signature/effect projections rather than treating reverse edges alone as semantic incrementality.

## Stage 9: consume current summaries at call sites and reduce hash preparation

User requests existing broad Rust comparison without new tests for that summary, then continued optimization. Existing Stage5/6 broad and Stage8 focused results remain separate versions. Baseline365c8a558; use two instrumented snapshots only for narrow warm-path diagnosis, not new broad confirmation. Current diagnostics show machine scalar postprocessing around40–50ms and object-key costs; persistent module Types reuse remains a concrete but lower-priority design because full current Types is around50ms and old-candidate decode has real cost.

Root owns emit.rs/current scalar_summaries.rs and focused driver controls for same-request zero-argument direct-call summary consumption; stage8_frontend_audit owns only object_cache.rs and its embedded controls for byte-identical buffered object hashing. Record proposals before implementation. Existing backend writing scope applies; append tools/research/incremental-compilation/stage9*, docs/research/incremental-compilation/STAGE9*, evidence/research/incremental-compilation/stage9/**. Independent reviewer stage7_scalar_review remains separate from both authors. Preserve exact current Verify/bind_lir, effect/trap fallback, source budgets, public Wire/Cargo.lock/product gates and default options. No main merge or environment publishing; no repeated full matrix for the existing-results summary.

Intermediate1bbc0baad passed16 controls/Clippy/independent review and75 requests/native. Target warm improves10.1% default/4.6% closed, edit only1.6%/6.6%; continue with changes/research-closed-summary-collection.md to avoid materializing known scalar definitions that the closed path later deletes. Root additionally owns existing backend build.rs handoff and emit/scalar_summaries eligibility, with actual-source closed/address/effect controls. Whole current Verify and emitted-model preflight remain. Prior two windows keep intermediate identity; no retest of broad historicalRust/check/O0 matrices.

0ecf97386 passed19 distinct controls, but its75-request window revealed collection did not trigger on parameterized leaf families. Keep that version/window as ineffective, not a collection gain. The amended proposal uses a current post-fact call graph, with all unsupported/non-scalar bodies retained as roots, to omit only unreachable emitter-supported scalar functions. Parameterized helper positive control added; full current Verify, unknown diagnostics and all existing guards remain. Confirm mechanism on one real large source before any further timing window.

d2994e257 mechanism3/native/currentVerify and54 measured requests pass, with target closed edit paired .650/newRust .807, but stable-name compatibility must be closed before delivery. stage8_frontend_audit now owns machine_stable_identity.rs only: original prepare retained, new collected-view helper filters only present definitions/hints and permits one owner while reusing full current-model preflight. Root wires build.rs, runs unchanged object_cache212::o2_folded_results_and_imported_bodies_preserve_current_verification, and binds final measurements separately. Existing stable symbols/insertion object reuse are required; no assertion weakening.
