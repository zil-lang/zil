# ZIT 全文覆盖补遗

独审：`/root/incremental_review`。逐段阅读原上传 `upload/zit.ts` 全部 731 行；SHA256 为 `719a914356a24ac1edbeb1ffe6637d22311c9299e6be9ddbcdf3f7fdddf1474f`。以下补充原 `ZIT-QUERY-REVIEW.md`，属于静态源码结论。

## 导出覆盖表

| 范围 | 已核对导出；主要内部机制 |
|---|---|
| 8–114 | `SB/BSC/ITR/KPU/SIG/OBS/RAW/PRX/LEN/$hk`，`Sig/OSig/ASig/MSig`，`$/sig/isSig/raw/sub/tog/toSig/bond`；`read/write/recalc/trig`、双向边与游标清理 |
| 18、343–364 | `reg/ent/onClr/gc/clr/runx/nox/scpx/spy/rxFn`；父子 owner、延迟清理、追踪窗口 |
| 71–267 | `abort/retry/AV/AP/AE/APE/SKP/fb/pnd/onTx/cancel/tx/action/tent/isPnd/isErr/isOld/latest/until/optim/withTimeout`；异步状态、代次、日志、提交与回滚 |
| 270–304 | `$q/schMac/hq/lq/flush/idle/tick`；高度桶、四优先级队列、微/宏任务 |
| 19–20、305–465 | `frg/txt/add/att/aft/bef/ins/emp/rem/swp/domx/attx/dir/kmap/rxProps/nsTags/useZit/tags/svgs/maths/ssrTags`；DOM 区间、属性、事件、样式与 SSR 接口 |
| 466–600 | `rx/$trap/hSwp/hClr/ixr/gpr/amu/mmu/afx/_amu/_mmu/_rst`；代理、容器/长度/键信号、行 owner、变更描述与重放桥 |
| 602–729 | `useArr/useMap/useMut/useDOM/useReset/reset`；数组操作、Map 点订阅、整体替换的键对位与旧壳复用 |

## 对编译器有用，但须改变合同的机制

**异步完成与版本。** `aCalc`（122–138）在 Promise 完成、流的每次产出及异常处核对捕获的 `vn` 和事务 `abt`，拒绝旧代写回；`abort/retry` 升代，`gc/clr` 使节点失活。`aDone` 在值未变时仍通知 pending→available，提示编译器把“结果相等”与“任务完成”分开。可用于编辑后晚到的解析/查询结果；发布时仍须核对当前请求版本、输入指纹与取消状态。`withTimeout` 只尝试调用可选 `cancel`，不保证终止底层工作。这里的全局追踪窗口在函数返回 Promise 后即恢复，**不能自动记录 await 后的读取依赖**；并发 provider 应显式携带查询上下文。

**事务与预算。** `tx/tent/action`（167–244）具有延后写、乐观写、显式确认、跨 yield 恢复写会话；`txStep` 只提交或回滚仍拥有当前 `s.op` 的操作，回滚逆序执行。可参考“本次结果与依赖一起提交”的所有权检查；不能把后写覆盖规则当编译冲突解决。`pn/pnd` 统计未决异步工作，**不是引用计数，也不是 token/node/CPU 预算**。`optim` 仅以当前值相等决定回滚，编译发布应使用版本身份，避免同值误认同一次写入。

**生命周期与错误。** 本文件没有 `pin/unpin/refcount`；实际是 `reg/onClr` 将子任务和清理项挂到 owner，`gc` 先失活再延清，`clr` 退订并清子项（18、343–364）。可参考请求 arena 的整域释放；不能据此设计共享查询的驱逐或跨线程存活证明。`runx/nox/scpx/recalc/mnt/fb` 用 finally 恢复上下文，但用户清理回调和迭代器 `return` 并非都由非抛异常清理链包住。`recalc` 可记录错误后继续 flush，finally 还会裁剪依赖（74–88）；`fb` 可保留旧画面。编译器必须将失败与旧成功分开，失败计算不能发布新的依赖集合或继续授予有效结果。

**集合与整体替换。** 对象读取按键、`ownKeys` 按结构、数组长度/索引、Map `get/has` 按键、枚举按容器订阅（466–527、666–676），直接启发符号查找与名字集合分开建边，缺失查找也记录边。但对象没有 `has` trap；Map 的 `has/get` 共用值信号，符号表须另区分“不存在”与“值为 undefined”。没有 Set 专用响应式适配。`reset`（682–729）先匹配首尾，再用 Map 对位中段，复用对象壳和行；可参考模块/声明稳定身份，不能直接原地修改已核 AST/IR。其键匹配未解决重复键，`patch` 删除字段表现为赋 undefined，均须重新规定编译语义。

## 不直接迁移与未执行范围

没有名为 `batch/untracked/scope` 的导出；实际对应队列合并/`tx`、`nox`、`scpx`。四 lane 与 DOM/SSR 挂载是浏览器调度；同高度不证明线程安全，`$w/$b/$tx` 也不是并发上下文。`raw/nox`、直接外部写入和 await 后读取可能绕过依赖；文件、环境、工具与规则必须显式入图。

`./__`、`./cmd`、`./zss` 未随此单文件提供；命令撤销重放、SSR、DOM、异步与生命周期这里只作静审，**未运行完整集成验证**。旧脚本的五组同步数值 probe 含 utility 替身，不能扩称本补遗所列功能均已测试，也不能推导编译提速。
