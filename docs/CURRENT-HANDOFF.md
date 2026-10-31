# 当前接续入口：施工9 / 228–229（待完整CI）

当前联合实现为 `04fbd476fdbf02fbeaee374d2a8895f16f23c954`。有限生成类型声明228、普通泛型where Copy229已完成交叉非作者Pass、真实LLVM API事件消费者和root 26/0/0受影响回归；工单仍待验，完整CI与main实际验收尚未完成。当前范围、原Fail和预算返修见 [LANGUAGE-228-229.md](LANGUAGE-228-229.md)，任务见 `.task/impl/228.md`、`.task/impl/229.md`，状态见 `data/construction-progress.json`。

本轮从远端6ba0b04e接续并保留施工8至06b27a2f的工单更新。225–227和编译研究属于并行施工线；本轮不修改其实现。两片未自动完成完整1.4、ZUI、B0或Rust产品门。施工日志只更新zil-log的“zil施工9”专属行，北京时间约10分钟一次；仅记录真实活动，收尾标记停止状态。

以下保留222–224上一批正式验收与历史失败记录，数值属于该批，不计为228–229当前CI结果。下一次接续应先核真实GitHub main，再读本入口和新报告。

# 历史验收入口：施工8 / 222–224

接续前重新读取真实 GitHub `main`。施工8从 `a8e8831a9e28ac2ec83be6eac740a8f04a00af7d` 接续已验收的218–221；本批222–224完成的是有限语言实现、原失败返修、分片非作者审查与下列正式验收。任务、状态和证据入口分别为 `.task/impl/222.md`–`224.md`、`data/construction-progress.json` 与 `evidence/integration/222-224/`。

## 正式验收身份

| 验收身份 | 实际提交 | GitHub run |
| --- | --- | --- |
| PR 完整验收 | `4d3df5891aafebdd8337337180a7e2559daec361`（实际 checkout） | [run 41088376020](https://github.com/zil-lang/zil/actions/runs/41088376020) |
| main 合并后完整验收 | `ccae8c60c96a2c174b137b2fab084e9e8a97a02f`（实际 merge） | [run 41106864272](https://github.com/zil-lang/zil/actions/runs/41106864272) |

最终 main 默认 workspace **2861 passed**，reference workspace **2826 passed**；正式通过还要求其余必需 workflow 步骤完成并成功、原始证据上传完整，不能只看默认 workspace 一项绿色。

PR9 最初合入 `39e52ef784d41f5744b909a9e6702ab16c1b1227` 后，[main run 41101248493](https://github.com/zil-lang/zil/actions/runs/41101248493) 保持 **Fail**：默认2860/1/0，reference2826/0/0。唯一失败发生在 `review223_machine` 向子进程stdin写入时的 `BrokenPipe`，早于退出码/stdout/stderr的采集；该次消费者输出没有被验证为通过。原 `entry_error.zil` 不读取stdin，修复 `e38ce27fda432e7a55b4a203f4fd047756d9512e` 将无输入样例改用空stdin且不写入；需要模式字节的样例仍严格检查写入。相对PR候选，生产源码、规范和源fixtures保持原字节，退出/输出/分配清理oracle不变。上表最后main是非作者复核该测试驱动修复后的新合并身份，没有把原Fail改名为Pass；PR与最后main的完整文件树不同，差异限于该测试驱动、223派工和修复证据，见 `evidence/integration/222-224/closeout/POSTMERGE-REPAIR-BINDING.json`。

本地 clean 132/0/0 与共同实现的19项受影响回归属于有重叠的两轮，不能相加，也不能替代正式 CI。旧218–221的2817/2785不计为本轮结果。

## 本批范围与下一步

222提供有限源级 `#for`，223提供完整嵌套 Product 的 Store 元素资格和失败清理，224提供完整 Product 的同步共享借入。组合行为、真实审查缺陷及原字节重放见 [LANGUAGE-222-224.md](LANGUAGE-222-224.md)。生成器仅展开普通函数和已支持语句；普通 `defer` lowering 仍 Unsupported。`&Product` 的闭包捕获（含现有立即调用形状）、拥有捕获 `Store<Product>`、Product 内 Store、递归 Store及可变嵌套 Product仍未开放。

原29份 readiness 输入现为15成功/12 Unsupported/2原语法无效，旧成功零回退；仅完整 Product Store 和精确 Unit/Product 协议两份原输入晋升。该旧矩阵没有 `#for` 或普通 Product 共享借入输入，也没有 native 执行，不能充作完整语言矩阵。

后续继续完整 Type/Syntax / `#emit` / 类型声明生成、更深拥有容器组合、一般 trait/lending、长期外部回调的关闭/注销/重入，以及挂起、取消和并发。B0仍为 Doing，181仍为施工；完整1.4与完整ZUI保持 NotSatisfied，Rust产品门与原生Windows保持 NotRun。四目标合取及可重复产品净突破未建立，不降低 `validation/product-gates.json`。

## 本轮实际交接检查

在已提交的收尾版本 `1b895ecb866e8c957cc020fe69bddf21979712f7` 上实际运行 `node tools/handoff.mjs`：全局 **Fail / 1356项记录完整性错误 / 0 warnings**。与修复后R的同环境基线逐项比较，错误多重集完全相同，新增0、减少0；222–224均为“已合入”且各零错误。原全局Fail继续保留，不能与施工7另一快照的1138项相减归因，也不代表本轮编译器测试失败。原输出、退出码和比较见 [handoff-final/RESULT.json](../evidence/integration/222-224/closeout/handoff-final/RESULT.json)。

`data/handoff.json`是上述已提交版本的可移植快照；它记录实际检查时的提交，不冒充包含该快照文件的新提交自身。冷启动仍使用实时 `--check`，以当前真实主树为准。最后的资料提交另有GitHub主线CI，正式语言验收身份继续固定为上表的R。

## 历史与接续约束

原run `41082643919` 的 Fail、`41084362447` 的默认3失败及reference Incomplete保留；91609072临时运行与e3e231b / run `41088078934`保留Superseded。原作者/独审失败、缓存污染的 Infrastructure/InvalidEvidence、限定返修与最终通过分列，详见本批总报告。

一单一branch/worktree/target，禁止跨工作树复用Cargo target。作者、非作者、PR实际checkout及main实际merge分别记录；主线合并保留审核提交，不改写历史身份。整仓验收核对全部workflow步骤以及小receipts与各阶段native原始artifact，不以缩减测试或丢弃失败产物换取容量。

冷启动运行 `node tools/handoff.mjs --check`，再读取主树规范、总目标及派定工单。历史handoff缺项与本批新增问题分列；稀疏checkout未展开旧evidence可能额外报缺文件，不能据此重标历史成绩。规范输入注册表使用 `python3 tools/update-indexes.py` 生成；旧片继续锁定其实际implementation_commit规范，不追改旧hash。

`research/compile-performance`为另一独立施工线；本批不移借其进度或声明相对Rust的编译性能收益。旧209/136/137交接内容保留在Git历史，不再代表当前入口。

施工期间只更新 [zil-log](https://chatgpt.com/space/page_5799fccae05c8191b25b68c26ec7a4ab) 的 **zil施工8** 一行，使用北京时间；收尾明确标记阶段完成。日志只记录实际活动，不承诺会话结束后继续后台运行。
