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
