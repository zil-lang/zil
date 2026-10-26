# 图编译研究：另一条对话或 Codex 的接续入口

## 共享什么

两条对话通过明确提交的文件交换设计、源码、输入、实验结果和反例。固定基线、独立分支、阶段结束后提交，再由另一侧读取和复现；不要依赖另一条对话自动取得未提交工作树、执行进程或完整聊天上下文。

截至2026-10-26，本轮共同研究源码基线为：

`ff0ddb7e59b2b0577c35b92e74b895c86f20b218`，schema/wire17。

这是已恢复并独立审核120/121/122后的真实源码。规范只读 `_zil_design.md`、具名附件和 `validation/product-gates.json`。旧111–119的未保存提交未恢复，不能从聊天摘要继承性能或完成状态。新的语言提交可以继续合入main，已开始的性能实验保持冻结基线，另列迁移后的结果。

## 已知研究结果与本轮分工

| 研究 | 可复现事实 | 裁决/范围 |
|---|---|---|
| 101 参数写事实图 | reverse128完整进程配对比0.7081507303 | 原95成本端点中89项未排除1%回退，Inconclusive，生产净回退 |
| 107 私有构造身份 | modules512完整进程配对比0.747634；全部19进程点估计低于1 | 原95成本端点中32项未排除1%回退，Inconclusive，生产净回退 |
| 123 本轮确认 | Kahn与严格向前图两候选；7280进程/29120请求固定确认，19等权1.002044、28等权1.003244 | Inconclusive；140单项136项上界超过1.01、没有单项下界超过1.01；生产净回退 |

历史记录在 `docs/research/101/`、`docs/research/107/` 与 `evidence/impl/101/`、`evidence/impl/107/`。不要把低于1的点估计等同于全矩阵非劣已证，也不要把未排除回退写成已经证实变慢。

根据123确认后的实际归因，外部会话现在优先研究：**重复结构编码/摘要身份（对照107），以及参数效果闭包的重复解释（对照101）**。可复现两者在同一基线的独立与组合收益，须重新走完整成本和增长合同，不能继承旧未准入结论。RPO/SCC与稀疏表示仍是候选，但先从真实源码证明热点，不能只优化raw逆编号图。共享研究结果不等于共享求证实现：Verify不得调用Types/Analyze的安全求解器；任何生产替换仍须自己的等价性说明、原生边界和独立审核。

## 123 给后续研究的实际约束

正式确认的140单项来自旧95端点加三族16/32/64增长的45端点，另两个固定等权汇总组成142族；全部7280进程、29120请求和603200源摘要由非作者独立复算。19等权的区间为[0.993323349, 1.011756725]，28等权为[0.996517910, 1.009827500]。局部transfer减半没有证明完整提速；采样没有删样或换样。实际结论与最终源码身份查123收据及独审。

确认之后的44个带探针诊断臂只用于归因，不并入上述时间证据。BASE冷请求的一次观察：modules512完整API约66.36ms，其中raw_digest约26.57ms、整个Analyze仅1.03ms；nested-if64约6.38ms，其中raw_digest约4.35ms、Analyze约0.272ms；reverse128约26.92ms，其中Types effects约9.98ms。阶段可能嵌套，不能把所有phase相加；这是共享环境的一次诊断，不能据两臂这些时间宣称速度比例。它给出的具体研究优先级是先检查实际重复编码与效果解释，别继续默认初始化调度或dominators就是主耗时。

## 现在就能操作：本地恢复与附件交换

当前可直接采用已验证的恢复交付，不要求先开远端仓库：

1. 使用本轮答复所附完整文件索引中的 `zil-draft24-20261026-source.zip`、同前缀 `-restore.py` 和 `-history-delta.bundle`；本轮包包含123研究与124–127语言交付。再从[draft22基础文件索引](https://chatgpt.com/library?file_id=libfile_d64618f9165c8191bfe009c0ccc2a8aa&account_id=personal)取得基础历史的三个分片。本轮历史增量已累计上一恢复轮，不必先串接draft23增量；恢复报告列出SHA与命令。上一轮ff检查点不含123最终研究文件，不能拿它单独当本轮交接。
2. 在Linux或WSL的Linux文件系统中恢复完整Git仓库。当前实际工具链与原生测量是Linux x86_64；这份恢复脚本校验Unix文件模式，不能把本轮验证称作原生Windows恢复验证。源码ZIP单独可编译，但不含完整历史，不能把缺历史的handoff标Pass。
3. 在Codex打开恢复后的项目，先运行 `node tools/handoff.mjs --check`，再按主树AGENTS读取规范、任务和研究合同。创建独立worktree：

```sh
git worktree add -b research/external-cfg ../zil-external-cfg ff0ddb7e59b2b0577c35b92e74b895c86f20b218
```

4. 将本文件交给外部会话，并使用下节任务文本。若该会话只拿到源码快照，可先做只读分析和研究夹具，明确历史交接未验证；正式生产实现仍按仓库工单/独审流程接续。
5. 外部会话提交可复现代码后，在它的分支导出相对共同基线的增量：

```sh
git bundle create ../zil-external-cfg.bundle research/external-cfg ^ff0ddb7e59b2b0577c35b92e74b895c86f20b218
git diff ff0ddb7e59b2b0577c35b92e74b895c86f20b218..research/external-cfg --binary > ../zil-external-cfg.patch
```

把bundle及简短结果交回另一侧。接收方先 `git bundle verify`，再fetch到独立研究ref并审查；不直接覆盖main。Git对象身份和输入SHA必须对应；只发一段“快了X倍”不足以合入。

## 给外部会话的任务文本

```text
你负责ZIL图编译的独立并行研究。先读AGENTS.md、主树_zil_design.md、
_zil_goal.md、docs/research/EXTERNAL-CODEX-HANDOFF.md、101/107研究合同与结果。
共同源码基线ff0ddb7e59b2b0577c35b92e74b895c86f20b218，schema17。
另一条会话已完成123：Analyze初始化Kahn与前向特化的固定完整确认，
结果Inconclusive，生产净回退，原候选/失败/计数/反例仍保留。
优先复核107重复结构编码/raw_digest、101参数效果闭包，或从真实
源码剖面找到新的主要成本；RPO/SCC需先证明热点，不依赖raw逆序模型。
先给一个可证伪的具体假设和写集，
按项目工单流程在独立分支实施，不改main、不重写旧历史、不改旧性能门。
现有原始研究材料与负结果保留；旧111–119源码未恢复，聊天数字不能继承。
比较必须绑定baseline、candidate、harness、工具链、输入及binary SHA。
构建、预热、完整check/build、Verify、物化、失败恢复和内存成本分别说明；
固定权重并保留每个单元，报告n/2n/4n增长；两边都完成才计算耗时比。
性能探索与确认分开，不反复采样直到过线；不得用平均掩盖真实消费者退化。
不要扩大Roam平台。交付源码提交、实验脚本、精简机器结果、反例与原件摘要，
并导出相对共同基线的Git bundle或binary patch。生产准入由另一位审核者决定。
```

## 长期协作：私有仓库与明确同步

推荐共用一个用户指定的私有Git仓库；每个会话独立分支/worktree，一个集成者维护main。每个阶段提交 `changes/`、相关 `tools/research/` 和 `docs/research/`、紧凑的 `evidence/.../result.json` 与反例。另一侧fetch该分支后读取新文件，继续实验或独审。实验期间不要悄悄拉新main改变基线。

当前原仓尚未配置remote；没有向任何未指定仓库推送。原历史包含约162 MB、180 MB和579 MB的实验归档，超过GitHub普通Git单文件100 MiB限制。正式接入前需安排有保留副本和提交映射的LFS/外置证据迁移，不能只在最新提交删除文件便宣称旧历史能正常推送，也不能强推重写唯一原件。完整历史和原始证据继续可恢复；轻量源码、合同、反例与结果摘要应便于每个会话取得。

最小实验交接字段直接复用现有研究收据：源码base/candidate提交，规范摘要，工具链与目标，命令和输入SHA，权重/样本/停止规则，实际状态/失败/计数，全部时间与RSS原值，生产准入裁决、未测项和下一条已具体化问题。不同机器的绝对毫秒不能直接相减；各自在同机同条件下运行baseline/candidate，再比较比值及其适用范围。

OpenAI官方文档说明：Codex worktree支持同一项目的多个独立会话；Cloud新任务从已发布环境创建各自隔离工作区。共用环境用于复用工具和依赖，源码及实验同步仍以明确提交为准。

- [Codex Worktrees](https://learn.chatgpt.com/docs/environments/git-worktrees)
- [Codex Cloud environments](https://learn.chatgpt.com/docs/environments/cloud-environments)
- [GitHub large-file limits](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github)
- [Rust MIR dataflow source](https://doc.rust-lang.org/nightly/nightly-rustc/src/rustc_mir_dataflow/framework/mod.rs.html)
- [Clang dataflow worklist source](https://clang.llvm.org/doxygen/DataflowWorklist_8h_source.html)
