# zil编译6 增量编译与查询发布优化

日期：北京时间2026-10-31。基线固定为编译5最终研究交付 `485301e1f1dcdfb0b4e24f6249355104243c3fe3`，crates tree `b583bc62e4aa39b206afb27f76f2f8262120fe9d`。本轮由用户明确授权继续编译专项、参考上传zit.ts、增加增量投入、使用全部可用代理并更新GitHub及zil-log；长期研究分支保持独立，既有Hold和历史负结果不改变。

## 机制与写集

CP21由 `/root/toolchain_recovery` 实施：历史084已在schema14研究过同类新键shortcut并判NoGo，本轮是在新基线重测，不是新理论发现。新QueryKey不存在合法存活依赖者时跳过无效全表失效扫描；已有键与真实依赖变化仍走原闭包规则。写集为 `crates/zil-db/src/lib.rs`、`crates/zil-db/src/tests.rs`、`crates/zil-db/examples/compile_performance.rs` 和本片证据。既有扫描oracle与可达DAG/驱逐对照用于限定正确性；与旧CP01反向索引候选分开记录。

CP22由 `/root/zit_analysis` 实施：在现有显式 `--query-cache` 的trusted-local范围内按模块复用成功解析。解析输入身份与当前剩余token/node预算分开，成功收据同时覆盖lexer token数、parser节点费用和attribute validator工作量；请求继续按原顺序收费，收据不足时回原解析器保持首错。FileId、ModuleId、路径、源字节、解析规则、深度与其它必要配置保守绑定。写集为 `crates/zil-syntax/src/{lib.rs,parser/mod.rs,attributes/mod.rs,cache.rs}`、`crates/zil-driver/src/{lib.rs,session.rs,project.rs,parse_cache.rs}`、两crate各自 `tests/parse_cache_cp22.rs`、`docs/research/compile-performance/CP22-PARSE-CACHE.md` 和本片证据。编解码增加内部语法事实，不更改公共IR schema、语言接受集、预算、Cargo.lock或独立Verify职责。

引用zit的具体机制为按键依赖、避免无关输入污染缓存键、重复计算的结果复用；不复制全局执行时钟或异步错误吞下策略。本轮语法缓存不等于已经实现函数签名/效果的语义等值截断：Expand、CT、Types仍按完整项目键处理，后续须解决稳定FunctionId、负查找、泛型/CT函数体依赖和资源效果摘要。

缓存发现阶段只读；每次重新读取源文件并解析导入的当前文件系统事实。ArtifactPlan通过及整个请求成功后才创建缓存目录/原子存储。损坏、截断、旧producer、低预算、未知变体和解码上界超限均回到当前冷路径；失败与取消不发布。缓存只保存未核AST，无Verified能力。producer身份每请求至多计算一次，不逐模块hash编译器。

`/root/incremental_review` 独立审核实现和测量，绑定精确提交，不参与实现。根协调者管理计划、组合、Git发布、受影响验证和完整进程计量。分树只读本主树工单，父任务仍为B0，研究通过不替代正式语言/产品验收。

## 验证与测量

先验证成功/失败AST编解码、所有语法变体覆盖、token/node/深度边界、低预算首错、模块ID重排、删除文件和同名文件/目录歧义、损坏/截断缓存与取消后重试。真实CLI使用独立进程，共享缓存与新空缓存对照；比较诊断和原生oracle，同时记录解析命中/缺失/存储计数、实际Verify和产物身份。

沿用既有五语料权重5/15/30/30/20和四请求权重10/35/40/15，不改正式门。先分别测CP21和CP22，再测组合；完整CLI外部wall/CPU为主，不能用从source discovery之后才开始的内部report.seconds评价解析缓存。测量期间不并行本轮构建或压力测试。Rust仍固定1.99.1（内置LLVM23.1.1），ZIL使用LLVM/Clang/LLD23.1.4；两方原生策略差异、CPU配额及冷缓存定义明确记录。

恢复环境只物化本轮需要的源码和测试，Git树与浅边界提交逐SHA核回，未下载历史大证据和父提交不记为完整历史恢复。工具链由官方校验包恢复；若使用从同版本官方静态组件链接出的x86 LLVM共享库，须单独记录该派生事实。历史handoff失败不掩盖，不把读取成功当作产品通过。

每约10分钟只更新zil-log的zil编译6一行，使用真实北京时间；本轮完成或受阻则记录实际终态，自动任务最低一小时间隔不能用于10分钟在线证明。四目标沿用现有RUST-GOALS与product-gates，结论分别说明安全、运行/内存、表达/维护和冷/增量编译的实证范围。

## CP23 独立读取快路径

在独审源码分析后派定CP23：缺失查询与无依赖叶查询保持active-cycle检查优先，直接返回，避免构造不需要的DFS工作表。所有非叶查询继续原完整依赖闭包检查。作者为 `/root/toolchain_recovery`，独审为 `/root/incremental_review`，从同一485301e1基线独立分支实施；写集为db的lib.rs/tests.rs、新example query_read_cp23.rs及对应证据，单独和组合计量，不与CP21共享收益归因。完整bond动态边/直接dirty frontier/高度调度仍单独研究，不把这个读取快路径称作已实现完整响应式查询。

## CP24 对称增量桥接

原旧新ZIL桥接默认只开source/object缓存，不能隔离CP22；增加显式 `--query-cache` 选项，使A/B两臂同样启用前端缓存，默认路径不变。计划冻结该布尔值、完整argv及query/parse报告，summary核对请求状态和原始日志。五语料、四请求权重、ABBA顺序及估计器原样保留；这是单独窗口，不与历史关闭query的结果合并。作者为根，独审仍由另一代理进行。

CP24已由非作者绑定 `c24cc08333dfb4ee710a7443414093f82841e0eb` 独审Pass：67项原有/新增Python测试及18项独立控制（含24种命令组合）。这是runner一致性证据，不是性能结论。旧计划仍须匹配原runner哈希；真实窗口另外核对query状态与parse命中，不能用requested=true替代实际缓存生效。

CP22写集补充：仅更新 `crates/zil-cli/src/main.rs` 的HELP描述，准确说明既有 `--query-cache` 同时支持逐模块syntax memo与frontend checkpoint，不更改CLI参数行为。最终release、独审与性能窗口均绑定带该说明的精确源码和二进制。

Rust对照新增窗口明确固定 `--rust-profile native --rust-lto native --cache-mode incremental --query-cache`，ZIL继续8units/4jobs、两CPU亲和。rust_bench只更新cache_scope与help文字以涵盖syntax memo，命令/模式校验/权重/估计器不改。原始stdout保留并hash绑定parse_cache；额外逐case/request审核hits/misses/parsed/stored，Rust原生CGU/LTO和链接策略不改。

## CP26 独立缓存发布成本切片

CP22首个完整窗口240请求/768原生校验通过，实际解析命中经独审逐条核回，但wall B/A=1.0205131985、CPU=0.9917486312；私有编辑wall=0.9329474997，首次建缓存wall=1.3299064214，整体采用Hold。原数据不改。下一候选仅针对逐模块同步发布：先核对可重建trusted-local memo的既有合同，使用缓冲完整写入、flush、close与原子rename，减少每条sync_all和成功后的无效unlink；不承诺断电缓存耐久。丢失、截断或损坏仍回当前冷路径；成功/ArtifactPlan/取消及key/length/checksum全部保留。CP26由原作者独立分树实施，非作者审核，单独冻结后与CP22及原基线分别计量，不从跨窗口差值直接归因。

CP26独审在CP22既有发布边界发现并用冻结binary复现：Temporary guard先于create_new构造，临时路径已存在时会误删并非本次创建的文件；真实output路径碰撞可导致CLI报Success但已发布ELF被删除。CP22完整发布正确性Pass因此撤回为NoGo，原106/10/94/60控制和240行性能仍保留其各自事实。CP26同一发布写集内把guard移到create_new成功后，rename成功后再disarm，并加入sentinel和真实ELF碰撞对照，不修改输入/预算/codec。

## CP27 既有缓存临时路径所有权修复

非作者在原485301e冻结CLI分别复现CP12 frontend checkpoint与LLVM object cache同类问题：确定性临时路径已经是本请求刚发布的ELF时，create_new返回AlreadyExists，旧清理仍删除未拥有路径；报告仍为Success且Verify=1。原始脚本、stdout、ELF存在性和运行对照单独封存，不归因给CP22，也不把先前测试改写为覆盖了此边界。

CP27由 `/root/toolchain_recovery` 从CP26精确源码d8872b52另建分树，独审仍为 `/root/incremental_review`。生产写集仅 `crates/zil-driver/src/query_cache.rs` 与 `crates/zil-backend-llvm/src/object_cache.rs`，补对应有意义的失败与真实产物回归、CP27文档/证据。create_new成功后才接管删除权，后续错误只清理自己的临时文件，rename成功后解除清理；不改变codec、key、资源计费、接受性、Verify或持久化策略。限定同类审查共4处：raw ArtifactCache已有正确open后arm顺序，未见此缺陷；CP26仅修parse，其余两处由CP27完成。

CP26与CP22的正常路径性能隔离仍单独计量；最终可用组合另与485301e和原生Rust配对，不能把不同窗口比率相乘。任何候选需先修复实际产物丢失问题，保留原失败和负收益，主线采用仍Hold。
