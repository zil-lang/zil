# CP22：按模块复用解析结果，保留顺序预算

基线：`485301e1f1dcdfb0b4e24f6249355104243c3fe3`。作者 `/root/zit_analysis`，独审 `/root/incremental_review`；父任务为主树 `.task/research/compile-performance.md`。本片不更改语言接受性、公共 IR schema、Verify 职责或性能门。

## 为什么

原 `load_sources` 和 `project_pipeline` 把前序模块扣费后的 `max_tokens/max_nodes` 放入后续模块 Parse 查询键。前面一个模块增加注释或表达式，就能改变后面所有模块的键，即使后面源码完全未变。原磁盘 source cache 仅保存源码，跨进程仍需重新词法分析和解析；长驻 `CompilationSession` 的收益不能当作 CLI 增量收益。

本片借鉴上传 `zit.ts` 按键订阅的思路：一个模块的语法事实只绑定它实际需要的输入，项目累计额度在使用结果时单独核对。没有移植 zit 的全局时钟、吞异常或高度阈值环判断。

## 做什么

解析专用身份包含 FileId、ModuleId、路径、完整源字节、规则/schema、编译器版本、深度、其余 Limits 和显式 build inputs；只归一化 token/node 两个额度。其它阶段的请求键仍绑定完整原始 Limits。磁盘键另外绑定当前安装编译器身份，沿用 CP12 的 trusted-local 前提：安装编译器和私有缓存生产者可信；摘要用于发现损坏，不认证恶意写入者。

`ParseResult.usage.nodes_required` 记录 `max(parser.nodes, attribute_validator.work)`。两者是原有独立额度检查，不能以 AST 节点数或 token 数代替。token 费用取完整 `tokens.len()`，包括 EOF。命中要求：

- 完整成功语法，不含 diagnostics。
- token 数不超过当次词法额度；项目词法额度仍是剩余 token/node 的较小值。
- `nodes_required` 不超过当次 parser/attribute 节点额度。
- 深度、字节及其它未重放轴由身份绑定；解码额外检查源字节限制。

不满足时调用原解析器，按当前剩余预算产生原首错、位置和后续诊断，不合成新的额度错误。每个模块之后仍调用原 `charge_frontend`，按原顺序分别从项目 token 和前端工作额度扣 `tokens.len()`，不把项目收费改成新的 receipt 数。

## 跨进程路径

只在显式 `--query-cache` 下启用磁盘语法复用，目录为 `parse-cp22-v1`。首版限定 Linux host、无 Windows target/link 选项、无 LD_PRELOAD/LD_AUDIT 注入；纯 `--stop-after parse` 保留原单文件路径，不发起导入发现。

每个请求只准备一次生产者身份。准备和 source discovery 只读，不创建目录、不写文件；未存在目录作为未命中。每次仍读取当前源码、收费并重新执行 `resolve_file_module`，因此删除文件、目录/文件同名歧义、路径越界和导入重排会走当前检查。身份保守包含当前 ModuleId/FileId，导入顺序导致 ID 改变时重新解析，不跨身份复用旧 span。

只有完整请求返回 Success、已有 ArtifactPlan 保护已通过后，才创建私有目录、编码并原子发布本次未从磁盘命中的成功 AST。取消前/后检查、临时文件清理及输入/产物路径重叠检查保留。既有 session 中失败请求已完成的前阶段仍按原行为保留；新增磁盘层不保存失败请求和半结果。

## 编解码与边界

完整静态 codec 覆盖 AST、所有 TokenKind/Symbol、deferred_attributes 和 cfg_removed；枚举写端 exhaustive match，使新增语法变体需要显式更新 codec。没有 Debug 文本反序列化、反射或 unsafe 字节重解释。

传输 payload 上限 32 MiB；解码累计声明分配上限 64 MiB；结构递归最多 128 层。每次写入/扩容前检查上限；Vec 长度先检查剩余字节及累计分配，再使用 `try_reserve_exact`。String、Box 同样收费。每个 span 校验当前文件的 UTF-8 边界与范围；根模块 ID/span 与当前 source 匹配；token tape 必须完整、连续、最后且仅最后一个 EOF。错误码、截断、无效标签、超界、损坏摘要、旧 producer 均回冷路径。合法深树超过 memo 上限只绕过缓存，不改变原 parser 的接受性。

新增 usage 元数据会出现在原 Debug 格式的 `syntax.txt` 中；候选冷/热 dump 应完全一致，此字段与旧版 dump 的差异不是语义 IR 变化。

## 可观察结果与范围

`CompileReport.parse_cache` 独立报告 `requested/hits/misses/corrupt/quota_misses/stored/session_hits/parsed/reason/write_reason`。`hits` 是跨进程可用的磁盘 AST 命中；`session_hits` 为长驻 session 的 Arc 复用；`parsed` 是实际调用原 parser 次数。现有 query_events 保持原来的查询发布/复用语义，不能把它当作词法/语法函数实际执行次数。所有完整编译仍获得新的独立 Verify；不序列化 Verified。

本片是语法增量：源码未变模块可跳过 Lex/Parse/attribute validation。Expand、CT、Types 的完整项目键仍在，函数体编辑后的类型和安全分析并未通过签名等值截断复用。后续语义增量还需要完整接口/效果摘要、负查找依赖、泛型及 CT 函数体依赖。冷请求增加编码和磁盘写入成本，热请求增加解码/摘要/IO 成本，收益必须用两臂同样启用 query cache 的完整 CLI 外部 wall/CPU 评估。

## 验证

定向测试位于两 crate 的 `tests/parse_cache_cp22.rs`。覆盖源文本往返、receipt 边界、损坏/截断/巨大声明长度、深树旁路；driver 覆盖独立 session、前模块 token 变化、低 token/node/depth/bytes 首错对照、模块 ID 重排、缺失/歧义导入、损坏修复、取消与非法产物路径不写目录。实际执行结果与独立进程 CLI 序列记录在本片 evidence；未执行项不得写成通过。

四目标：安全与错误/预算语义通过对照验证；机器生成路径未改，运行性能不据此宣称提升；语言表达未改；编译收益待冻结候选的完整进程测量，不能以缓存命中数或测试数量替代时间结论。
