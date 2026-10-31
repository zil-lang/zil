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
