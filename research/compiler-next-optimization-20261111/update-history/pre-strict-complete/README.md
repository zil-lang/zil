# 编译优化下一轮：文本索引与恢复布局（2026-11-11）

本目录是固定归档的轻量文本材料，不是新性能运行。Root负责后续Git提交/发布及原始证据单包持久化；当前raw包明确pending。旧`research/compiler-multi-edit-20261111/`未改动。

## 决策快照

| 对照 | 结果 |
| --- | --- |
| S：直接结构sink，发布ee0cc452／作者924076，同tree ad37913 | 84请求、42对、252短I/O完整且同ELF/materialIR；固定screen总wall +7.632591%，未建立收益，NoGo不采用、不扩矩阵 |
| B：builder插入块复用，发布9f0deb4／作者78e300，同tree257a296 | 同84/42/252完整且同码；固定screen总wall +2.078146%，完整请求和builder相位收益均未建立，NoGo不采用 |
| 默认ThinLTO：发布eb5b150／作者e2e3ce1，同tree1da99d0 | 6clean、54native、36formal完整、输出正确；n64/n256 B/A=1.7357655313/2.0076492560，均越1.10，NoGo，不扩矩阵 |
| 严格完整可见性：发布2549cbe／作者499798，同treeebb9931 | release构建、5个proof测试及无native的check现已Pass；新脚本/protocol冻结和质量运行仍待定，不填写运行或性能结果 |

S/B工作总量不是用户频率加权分数，短I/O不是runtime性能样本；Rust90及10%相对退化门、完整O2和Verify/预检合同未放宽。旧a60/1679、AE、CP084等否定记录不被本轮覆盖。

## 布局与恢复

- `snapshot/research/`：Rust/Blaze只读研究、机制审计、S/B源码/协议/结果复核及复算脚本；Thin默认正式冻结Python、vendor、process-scope工具、现存v1/v2/v3旧冻结记录及旧wrapper、工具pilot输入/结果。`thinlto-complete-visibility/`保留当时源码静审；新增2549构建绑定、release/5proof/check成功日志另在out索引；新脚本冻结后另补，不用默认Thin脚本冒充新对照。
- `snapshot/out/next-optimization/`：A/S/B/Thin binary与源码绑定、固定plan/attempt身份、结果summary/completion及默认Thin正式protocol。完整raw、ELF、BC、decoded大LLVM文本、工具二进制和cache等待单独raw包；仅v1两unit首败的小stdout/stderr与leaf.prelink.ll作为例外随文本保留。
- `manifest.json`：每份复制文件的原绝对路径、归档相对路径、字节数、SHA；源码remote与author local同tree关系；已完成/未执行范围；raw持久化pending。复制前后stat/hash稳定校验，不修改原物。
- `SHA256SUMS`：覆盖全部归档payload、README与manifest；该文件不自哈希，避免递归，根提交/外部收据可另记录其SHA。

先在本目录执行`sha256sum -c SHA256SUMS`检查文本，再将`snapshot/research`和`snapshot/out`按相对结构恢复到新任务根。源代码从manifest固定commit/tree恢复独立worktree，author local SHA不冒称远端已可达；不要用工作区basename猜源码。实际工具按binding/protocol的版本、调用角色和SHA重新获取，不能把缺失binary当已复现。

S/B用保存的corpus.py重建原语料并核plan六源hash；完整八语料可生成但本轮只执行原screen三语料/四lane。Thin用正式prepare.py/vendor从protocol定义重建n64/n256固定base及参考源，再逐文件核protocol中的source hash；生成.zil/.rs未重复入本包。运行脚本的冻结绝对路径不能直接换址后冒称原plan：移址应另记录恢复映射与新身份，不能伪造原attempt。历史attempt一旦消费不得复用/追加采样。

Thin `prefreeze.json`保持准备时原状态文字，正式执行事实以已完成protocol/result-review/summary为准；旧prefreeze并不都是运行结果。现存archive-v1/v2/v3仅有部分历史文件字节，manifest逐条列出只有旧hash而尚未定位的版本；**不把hash清单说成全部旧代码已经保全**。正式实际运行protocol绑定的13份v4 Python全部在本目录且逐hash核对一致，可按其冻结hash恢复。prefreeze/4的早期README已由Root精确恢复到archive-v4/README.md，SHA与旧freeze完全一致，restoration.json记录仅反转已知状态段替换的恢复方式；当前顶层README仍保留运行后版本。旧v1/v2/v3未定位字节仍维持metadata-only。旧缺失版本保留待补状态，不臆造内容。

本轮源码四个候选均未合main；最终归档commit/远端可达、raw包路径/hash/bytes和严格对照新脚本/结果仍由Root在完成后补齐，当前不填假SHA或结果。
