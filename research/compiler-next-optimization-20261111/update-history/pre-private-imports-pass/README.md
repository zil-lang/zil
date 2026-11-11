# 编译优化下一轮：文本索引与恢复布局（2026-11-11）

本目录是进行中的轻量文本证据快照，不是最终档案，也不是新性能运行。下一项私有导入 pass 实验正由作者实施，其源码/冻结协议/运行结果尚未纳入；不得以本快照宣称该实验已完成。Root负责后续Git提交/发布及原始证据单包持久化；当前raw包仍明确pending。旧`research/compiler-multi-edit-20261111/`未改动。

## 决策快照

| 对照 | 结果 |
| --- | --- |
| S：直接结构sink，发布ee0cc452／作者924076，同tree ad37913 | 84请求、42对、252短I/O完整且同ELF/materialIR；固定screen总wall +7.632591%，未建立收益，NoGo不采用、不扩矩阵 |
| B：builder插入块复用，发布9f0deb4／作者78e300，同tree257a296 | 同84/42/252完整且同码；固定screen总wall +2.078146%，完整请求和builder相位收益均未建立，NoGo不采用 |
| 默认ThinLTO：发布eb5b150／作者e2e3ce1，同tree1da99d0 | 6clean、54native、36formal完整、输出正确；n64/n256 B/A=1.7357655313/2.0076492560，均越1.10，NoGo，不扩矩阵 |
| 严格完整可见性：发布2549cbe／作者499798，同treeebb9931 | 6clean、54native、36formal全部完成且输出正确；n64/n256 B/A=1.6181061285/2.0561126545，均越1.10，NoGo；实际Required/AE=56/56与224/224，完整所需导入仍未恢复质量 |

S/B工作总量不是用户频率加权分数，短I/O不是runtime性能样本；Rust90及10%相对退化门、完整O2和Verify/预检合同未放宽。旧a60/1679、AE、CP084等否定记录不被本轮覆盖。

## 布局与恢复

- `snapshot/research/`：Rust/Blaze只读研究、机制审计、S/B源码/协议/结果复核及复算脚本；Thin默认正式冻结Python、vendor、process-scope工具、现存v1/v2/v3旧冻结记录及旧wrapper、工具pilot输入/结果。`thinlto-complete-visibility/`现保留v2全部冻结Python/vendor/prefreeze、源与协议审计、实际Required/AE名单及费用、正式结果、IR/inline归因；`thinlto-complete-visibility-history/`完整保留未执行v1及两行变量rename修复收据。严格actual protocol另存，绝不使用默认Thin身份冒认。
- `snapshot/out/next-optimization/`：A/S/B/Thin binary与源码绑定、固定plan/attempt身份、结果summary/completion及默认/严格Thin各自正式protocol。完整raw、ELF、BC、decoded大LLVM文本、工具二进制和cache等待单独raw包；仅v1两unit首败的小stdout/stderr与leaf.prelink.ll作为例外随文本保留。
- `manifest.json`：每份复制文件的原绝对路径、归档相对路径、字节数、SHA；源码remote与author local同tree关系；已完成/未执行范围；raw持久化pending。复制前后stat/hash稳定校验，不修改原物。
- `SHA256SUMS`：覆盖全部归档payload、README与manifest；该文件不自哈希，避免递归，根提交/外部收据可另记录其SHA。

先在本目录执行`sha256sum -c SHA256SUMS`检查文本，再将`snapshot/research`和`snapshot/out`按相对结构恢复到新任务根。源代码从manifest固定commit/tree恢复独立worktree，author local SHA不冒称远端已可达；不要用工作区basename猜源码。实际工具按binding/protocol的版本、调用角色和SHA重新获取，不能把缺失binary当已复现。

S/B用保存的corpus.py重建原语料并核plan六源hash；完整八语料可生成但本轮只执行原screen三语料/四lane。Thin用正式prepare.py/vendor从protocol定义重建n64/n256固定base及参考源，再逐文件核protocol中的source hash；生成.zil/.rs未重复入本包。运行脚本的冻结绝对路径不能直接换址后冒称原plan：移址应另记录恢复映射与新身份，不能伪造原attempt。历史attempt一旦消费不得复用/追加采样。

Thin `prefreeze.json`保持准备时原状态文字，正式执行事实以已完成protocol/result-review/summary为准；旧prefreeze并不都是运行结果。现存archive-v1/v2/v3仅有部分历史文件字节，manifest逐条列出只有旧hash而尚未定位的版本；**不把hash清单说成全部旧代码已经保全**。正式实际运行protocol绑定的13份v4 Python全部在本目录且逐hash核对一致，可按其冻结hash恢复。prefreeze/4的早期README已由Root精确恢复到archive-v4/README.md，SHA与旧freeze完全一致，restoration.json记录仅反转已知状态段替换的恢复方式；当前顶层README仍保留运行后版本。旧v1/v2/v3未定位字节仍维持metadata-only。旧缺失版本保留待补状态，不臆造内容。

本轮源码四个候选均未合main；最终归档commit/远端可达、raw包路径/hash/bytes仍由Root在完成后补齐；严格新脚本和本轮NoGo结果已经补入，当前不填假SHA或结果。

## 严格导入正式结果与精确诊断补充

严格固定协议 `snapshot/out/next-optimization/thin-complete-protocol-v1/protocol.json` SHA `5b0ff104485ba18c34db170716ead50cc438bfc3b8c8ebc1c1d1266876946728`；source 2549cbec8e1f3096d4c8c8a960459c83095ad13d/tree ebb993128355fed8228627c6d7b37d5cb0a90d45，binary fdfe6cea83286475582ce5b9063e9d84a9371b584baf99920193689f5e93d3e4。正式6编译/54native/36formal全输出正确，196process attempts正常；原始1192文件独审前后hash一致。两格分别B/A=1.6181061284580245/2.0561126545254256，Rust/B=2.3864832011975228/2.3620208027736953；后者通过不挽救前者失败。全部样本、长尾与getchar诊断保留，NoGo不是Incomplete。

`thinlto-complete-visibility-actual-audit.json`保留逐AE名称/owner/原函数费用：N64/N256原sourcecost35777/144449，原copycost31360/125440，unit0原Allowed504/2016，实际Required=Added=AE为56/224，未扩大预算。完整的是当前prelink所需函数体；预链接已内联的helpers不要求以每个原符号独立导入。14个非根unit未新增AE。原默认16imports全空NoGo仍独立保留，不被本结果覆盖。

`inline-diagnostic-report.md`及`inline-diagnostic-evidence.json`记录Root另授权的一次N64诊断重链：恢复正式8份prelink BC逐SHA相等，仅追加inline remarks，不运行产物、不新增正式样本、不评价其重链耗时。182条实际remarks=168 TooCostly+9 Inlined+5 NoDefinition；56跨owner AE三轮拒绝，7个同owner内部函数inline。LLVM23.1.4 local sole-call奖励15000与实际linkage/cost差异对应；fn14实际报告565/562并不证明完整cost只超3，不建议阈值扫描。显式range传播和overflow消除已发生，不能笼统说AE使全部IPO失效；同owner局部对照也不能证明私有导入新实验必过质量门。

诊断的原始小YAML、解析记录、plan/attempt、shape比较与artifact索引随文本保留；BC/ELF/decoded LL仍只在待打包raw内。本档案中的旧报告按其形成时点保留，早期“等待诊断/尚未复制”措辞由本节追加解释，不回写旧事实。所有原复制文件保持原位；若本次更新需替换旧内容，其旧字节另在`update-history/pre-strict-complete/`保留，连同旧README/manifest/SHA256SUMS。`source-copy-receipt.json`逐项记录只读来源before/after/副本SHA，原source没有修改。

## 退役与尚欠内容

`retirement-next-optimization/`四份占位草稿及独立审计已入档：只在成功composition模板上替换四字段，未执行workflow、未删除任何ref。PRIOR仍是A0/A1/A2/A3占位；新A0及顺序归档/删除需由Root实际完成并另存收据，不能把静审Pass当执行成功。

仍欠：本轮完整raw包持久化路径/hash/bytes，下一私有导入pass实验source/protocol/真实结果/非作者审计，最终archive Git提交与远端可达、最终退役状态及收据。默认Thin旧v1/v2/v3缺失字节仍只保留已知metadata，不宣称完整恢复；严格v1/v2则已逐freeze全部保全。本更新未运行编译器/LLVM/native、未采样、未git add/commit/remote。
