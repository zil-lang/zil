# 源码进度备份与接续

GitHub `task/language-mainline-20261103` 保存本轮集成源码；`task/322`、`task/323`、`task/324`、`task/325`、`task/326`、`task/328` 保存独立施工提交。分支中的施工源码未必通过验收，远端 main 只接收完成本轮审核和验证的候选。所有推送均核对 `git ls-remote` 的完整提交，禁止 force push、reset、clean。

每个阶段先将本轮已授权的源码、测试及必要输入 checkpoint 提交，批量 atomic push 后逐 SHA 核验。仍有未提交源码时，以独立临时 index 构造完整源码树快照到 `backup/wip-20261103-<号>`；保留原工作树和 index。快照明确 validation pending，不能把提交号或备份成功记作功能验收。后续快照保留前次快照为父提交，避免覆盖历史。

2026-11-03 12:29 UTC 批次已核验 8 个远端引用，覆盖6个施工提交、主树 `9cd66a3783835a5b2a7326884b2980230df0ef2a` 与325未提交源码快照 `ead8337dc0c215ef1f61da08d58c83995bce5304`。源码 bundle SHA256 `8b94301c598453620ce3583edcffd90288232ced7fb4887c41eae1832c3488a5`；独立 bare 恢复、完整 fsck 和12次源码blob比较通过。本地副本在 `/workspace/backups/zil-mainline-20261103/20261103T122916Z/`，环境丢失时以GitHub为异地来源；不将同机bundle称为异地备份。

恢复时先 clone/fetch 远端、检查所有 worktree、分支和未跟踪文件，再核源码对象及实际测试。`/tmp` 日志可能丢失；测试结果应保留精简、带源码和输入身份的已提交JSON，不归档大量原始证据。GPT 对话或资料空间不是当前源码备份通道。
## 2026-11-04 主线接续

329–334及旧范围复验已完成本地有限验收，编译器B=`14832d5a35630b7002c79e4df47b0cbb007570b6`、完整受测C=`65c25910616196eae76911afb13818dc79b73012`。GitHub进度分支仍为`task/language-mainline-20261103`，各施工和WIP源码已按阶段atomic推送并逐完整SHA核对；不要把分支名或元数据当作实际源码恢复证明。

本轮最终bundle与远端main/进度头、实际恢复checkout比对的收据放在`/workspace/backups/zil-gaps-20261104/final/receipt.json`。该收据在最终推送及合入后生成，不在源码报告中制造自身提交哈希。接续优先从GitHub实际main恢复，再运行live handoff检查；旧301–312缺失源码和legacy失败没有被追认。完整范围见`docs/LANGUAGE-GAPS-20261104.md`。


## 2026-11-04 后续施工336–339

接续时实时核得远端main和旧进度枝均为`e5a4d51d844e29c4f4f5184eaff5704b1b27dd24`，37个已有worktree全干净。新施工集成备份枝为`task/language-mainline-20261104`，合同起点`c4da43441b03661e61d0087000a13dbbe7a2a75a`已实际推送并逐SHA核验；各片源保存在`task/336`至`task/339`，在途未提交源码另以`backup/wip-20261104-<号>`保存真实Git树并保留前一快照父链。

阶段收据在`/workspace/backups/zil-batch336-20261104/<UTC批次>/receipt.json`。备份不改施工index或worktree，不运行reset/clean。当前合同/源码施工和最终接受分开；远端main只在独审及正式验证收口后更新。环境丢失时先从GitHub clone/fetch这些实际引用，读取源码与主树工单再执行检查。

本批正式受测源码`874f071bb53e85727b5467a2769ada47b51edd98`及所有作者/独审源码已进入main历史，PR32合入后将再次核对main与最新进度枝。最终收据固定路径`/workspace/backups/zil-batch336-20261104/final/receipt.json`，包含实际GitHub恢复checkout、源码文件逐字节比较和live handoff结果。当前接受摘要指针更新到本批精确结果，历史inventory不改；备份成功不把未实现功能或历史材料追认为通过。

## 2026-11-04 终态施工340–345

当前集成源码持续保存到`task/language-terminal-20261104`，作者树为
`task/340`至`task/345`，独审为`backup/review-terminal-20261104`。
精确隔离修复另保留`task/342-capturefix`和`task/344-typefix`，不要仅取
主施工树上相似补丁代替已审对象。20261104T042552Z批次实际核对15个引用，
集成头20f5aa591、343登记候选984a01d51、345插入候选d6e70ea96均可恢复。

主集成树本身的未提交修复现在也保存为`backup/wip-20261104-integration`；
该批实际树75673a0d5含正在修复的深表达式栈问题，**未通过全量验收**。
其余作者未提交源仍保存在`backup/wip-20261104-<号>`，每次使用独立临时
index且保留上一快照父链，原index、工作树和未跟踪源均不受改写。
所有引用在同一次atomic push后按完整SHA核对；精简收据位于
`/workspace/backups/zil-terminal-20261104/<UTC批次>/receipt.json`。

环境遗失时，先fetch上述真实分支，再检查对应Git树和各自验证收据。
已验收main仍是PR32；终态进度枝、WIP快照和独审范围均不得冒充全语言完成。
