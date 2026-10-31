# CP33 独立正确性审计

结论：**Pass，范围为冻结候选的静态设计与正确性控制**。精确提交 `83854ae83f7fec4fc8f9c22d0751a1fd2725671f`，基线 `ee72fd6f701efa5f7a75ea3f13d4effc20d5c4b8`。审核 cp25_evidence，未修改作者 worktree；起止均为干净状态。生产变更限定在 syntax 的 lib/parser/attributes 与 driver 的 session/project 五个文件。候选不包含 CP29 或 CP32。

本次未取得性能采用、原生运行或产品总门禁结论。实际测得的 Parse 命中改善仍须纳入完整请求与进程成本窗口，不能借用 CP22/CP26 的历史数字，也不能推断 CP32+CP33 联合效果。

## 收据能证明什么

原 parser.nodes 与 attribute validator.work 分别受同一个 max_nodes 上限约束。因此成功解析的充分 node 收据为两者的最大值；取最大值不会少算一个共享累加器，因为这里原本就不存在两项相加的共享累加器。新增代码只带出原计数，没有修改任何原预算分支、收费或诊断。

fits_limits 同时要求：无诊断、包含 trivia/EOF 的 token 长度不超过当前 max_tokens、nodes_required 不超过当前 max_nodes。EOF 的判断是保守充分条件。例如空源在某些 0 token 配额下原 lexer 仍能成功，receipt 可能不接受；候选会回到原解析器，而不是根据 receipt 自行生成错误。

成功 syntax 的私有查询身份仅将 token/node 两轴中和，并增加独立的 CP33 标记。源字节、path、FileId、ModuleId、规则/schema/producer、build_inputs，以及另九个限制轴仍被绑定。driver 的属性上下文保持原默认值。ParseResult/ParseUsage 虽是公开数据类型，外部没有把它们注入 driver 私有 raw 表的入口；本实现没有 decoder、磁盘 syntax cache 或新的发布协议。

当已有成功 receipt 不足时，parse_key 选择原完整预算键。缓存中若存在这个键的成功结果，只能在完全相同的额度下复用；否则仍执行原 lexer/parser，保留首错、顺序、Span 和部分工作。

## loader、后续阶段与失败路径

load_sources 和 project_pipeline 共同调用 parse_key，并采用相同有效 token/node 余额。loader 的源字节额度在文件读取时独立扣减；pipeline 仍验证完整 project。项目收费保持原 tokens.len，包括 EOF，不因缓存命中而省略。

每次请求仍重新读取源字节、解析当前 import 路径，并检查不存在的文件与文件/目录歧义。缓存 syntax 不缓存目录解析结果。loader 已验证的 Arc 可以进入 pipeline，但该路径没有改变当前源身份或剩余额度。

single-source 管线明确保留原完整 identity：后续 phase_key 与 source backup 使用它，只有 Parse 查询使用新 key。project 后续阶段也继续使用完整 project identity。CT、Types、Analyze、Verify 的额度身份没有跟着中和；前端磁盘 checkpoint 也未改变。

raw 发布仍只接受成功构造；query 的取消检查、失败 abort、active 清理与容量清退机制未改。Verify 仍是请求内独立执行，并不从 Parse 收据获得验证权限。

## 冻结后的实际控制

在根批准的正确性窗口里，jobs=2、测试单线程，独立复跑了作者的精确冻结测试，再将三个新公共 API consumer 链到同候选的 debug rlib。命令、日志与摘要全部保存。

| 控制来源 | 通过 | 失败/忽略/过滤 |
|---|---:|---:|
| 既有 syntax lib | 17 | 0/0/0 |
| 新 Parse 收据集成控制 | 2 | 0/0/0 |
| 既有 driver lib | 36 | 0/0/0 |
| 新 driver 额度控制 | 6 | 0/0/0 |
| 新非作者外部 consumer | 3 | 0/0/0 |

即 **61 项作者控制独立复跑 + 3 项新非作者控制**，不与作者先前同一批的运行数重复相加。新增 consumer 直接覆盖三个关键边界：

- **后续阶段仍受原额度约束。** 真实源在 max_nodes=24、Parse 收据=24 时，warm 请求复用了 Parse；CT 仍和 fresh 一样返回 `ResourceLimit / E-RESOURCE-LIMIT`，消息 `Core traversal depth/node budget exceeded`，首错 FileId(10)、span 57..58。没有后续阶段复用，恢复宽额度后重新 fresh Verify=1。
- **两个额度同时改变。** 四组真实 syntax（空源、UTF-8/嵌套注释、多个 doc 属性、cfg 删除与延迟附件）的 81 组 token/node 联合边界中，warm/fresh 状态、阶段、诊断、Span 与报告观察量一致。receipt 足够时，完整 AST、token、deferred/cfg 信息及使用量也逐字段等于新解析。
- **跨入口借用与当前路径解析。** 四个源分别通过 compile_source 以独立完整额度预热，再进入 compile_path 的逐文件剩余额度流程；报告显示 4 个 Parse 查询全部 Reused，并执行一次 Verify。随后新增 `c/mod.zil` 造成歧义，当前请求与 fresh 都拒绝为 `E-MODULE-AMBIGUOUS`，Verify=0、active=0。

这些控制核实完整报告/接口观察量和所述 syntax 值，没有新增 native 执行控制。Parse 查询事件并不总等同 lexer 实际调用次数：容量清退后，loader 借到的已检查 Arc 仍可被重新发布成 Executed 查询。因此最终性能分析应保留真实全进程成本，不能从命中计数直接计算提速。

## 复核入口与结论范围

机器收据为 `review/CP33-INDEPENDENT-REVIEW.json`；61 项控制命令在 `review/CP33-INDEPENDENT-AUTHOR-CONTROLS.json`，完整日志为同前缀 SYNTAX/DRIVER 文件。新源码 `review/cp33_independent_consumer.rs`；链接参数、rlib 与二进制摘要、退出码在 `review/CP33-INDEPENDENT-CONSUMER-EXECUTION.json`，原输出在相应 COMPILE/TEST 日志。

未发现正确性阻断问题。此项只复用成功、当前额度足够的进程内 Parse 构造；没有将私有 raw 查询变成完整增量语义编译器，也没有扩大原语言支持范围。完整性能窗口、默认采用结论与产品门禁仍独立进行。
