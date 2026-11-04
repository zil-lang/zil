# 语言施工进度复核与收敛

本次核对实际 Git、58 个 worktree、未提交/未跟踪源码和 111 个远端分支。
不按旧检查清单或提交号推断完成度。远端接受 main 仍为
`4bc1d77b5ec3bccb5b789fd813e85386f03ae13f`；本地主树是可运行的39/CT4
候选，40/CT5仍在独立集成分支，CAS41保持后继候选。PR34为草稿。

## 进展和实际瓶颈

| 范围 | 已有成果及本轮进展 | 仍需完成 |
| --- | --- | --- |
| 基础类型、模式及可靠性 | 原大数组O0/默认2MiB栈问题已在接受main修复；Option、记录枚举、部分移动、原生无符号链已有实际消费者。340拥有字段替换的生产者已在独立树。 | 替换的Verify/机器责任交接、贷款与pin等一般组合。不能把原大数组问题重复列为未修。 |
| life/trait及高阶调用 | `4d9d1b9ae`接通真实混合life/F helper；常量体及`p.n+*q`两源共8次Clang/API O0/O2运行，另有闭合Code参数4次。泛型33、组合83、Types44、Analyze13/Verify12分别通过。 | 捕获的外层Fixed上下文、qualified FnMut/FnOnce、lending结果和一般状态/别名组合。 |
| Type/Syntax、CT | 39/CT4混合const/life/type源已可运行。40/CT5接入真实Definition上下文、完整序号、上下文键和物理字节计费；344中断源码已恢复成`9cd6e354c`，在共同40组合为`1d939a646`。 | 完整实例实际参数向量、成员/生成作用域、rich callable与ambient Type导出的上下文消费者。 |
| 异步并发 | 静态拥有子任务、Unit及有限Copy错误结果已到机器；Task40计费修正`e714671bd`获独审`0b35ea609`限定通过，审查已归入主树。CAS机器消费者已有检查点。 | CAS41完整源码链、通用挂起/取消排空、外部waker/晚通知、一般Mutex/Share及责任组合。 |
| 动态抽象、库和平台 | dyn真实Source→Core声明身份、普通字符串/Store/UI基础接口已有源码成果。 | dyn异构执行/显式存储、完整标准库、ZUI平台IME/字体/渲染、原生Windows与Rust同语义成本验证。 |

最终共同40源码`7dfc6846194643e5295a7704a0b8ce99fa5176e4`的四个既有完整
回归目标分别aggregates149、cross_function_borrows84、generic_regressions83、
generics33，合计349通过、无失败、无忽略/过滤。六项CT5 Definition上下文控制
在组合前后均通过；CT库56项通过绑定344恢复树。两个实际CT5正例共八次
Clang/API O0/O2执行，其中原invocation-scoped phantom Code用例从Unsupported
推进到输出42；普通runtime变体的错误下标/错误结果类型经两核raw/Wire拒绝。
这是限定组合回归，不是整仓终验。

首轮aggregates140/9失败仍保留。八个失败来自旧Task集成0bccd16e8误删Option
原有拥有载荷例外；已以81a0667b7修复，主树39同步为022ad313e并单独通过50项。
另一项原源已越过旧Expand断点，继续补齐普通Copy聚合提取标量字段的两核和
机器消费者后通过；没有把新拒绝位置直接改写为功能完成。

当前主要瓶颈是公共表示迁移后的消费者衔接：多条分支各自的局部通过，不能
累加为同一棵源码的完整通过。此次直接接手341和342未完成消费者，再贯通
真实life/F Source→Types→Analyze→Verify→Lower→LLVM，已经消除一处实际
断点。推断后的Core再次进入Types时，不再把局部类型中的binder数量误当
原始源码显式引用外层life；原始显式注解仍在真实词法上下文检查，free Fixed
代码值和每份实际role仍有独立核边界。未用错误helper定义继续拒绝。

本轮life/F真实读取源的完整Source-through-Lower边界为N21782，N−1拒绝，
warm/fresh一致；静态Code旧N12633保持。Types/Lower/LLVM库严格Clippy通过；
Driver检查显式豁免已有的dead_code，其余warnings拒绝。新增改动是作者集成
证据，尚无新的独立审查和正式全量结果，不升级完整语言或产品门。

## 后续推进方式

先把已写出的消费者合到共同40，逐项运行真实源码、两核与机器行为；同时保留
39可运行检查点。顺序为life/F闭环、真实CT5 Definition数据、拥有字段替换、
CAS41与通用异步责任。每项交付带最近反例和适用预算，不继续只扩公共表示或
另开大量测试目标。阶段源码和实际WIP先集中备份，再做长时间组合检查。

六名施工子代理本次均因服务usage limit停止；全部未提交源码已经捕获并推送，
root继续接手关键路径。额度停止不等于云环境离线。GitHub此前CI任务未启动
是账户付款/消费限额问题，不能记录为代码检查通过；本地可构建、执行和推送。
独立审查恢复前保留候选状态，不降低既有验收要求。

## 分支清理与恢复入口

已删除63个完全被实际接受main包含的远端旧分支，移除40个干净旧worktree，
删除39个本地旧分支（另一个worktree原为detached）。删除前重新检查完整SHA、
祖先关系、工作区及未跟踪文件，删除后远端读回确认。没有reset/clean、强推或
丢弃在途源码。仍有独有提交、WIP或当前作用的分支保留；清理后18个worktree、
48个远端分支。精确清单见[清理收据](validation/branch-retirement-20261104.json)。

可恢复源码位于GitHub的`task/language-terminal-20261104`、
`task/terminal-public40`及各任务/WIP分支。批次收据在
`/workspace/backups/zil-terminal-20261104`，逐完整SHA核对，不把本地commit或
未上传bundle称为远端备份。本次重新运行`node tools/handoff.mjs --check`
返回`CurrentPass`且errors为空；该检查明确只确认本地主树，不冒称远端main更新。

Rust四目标仍按原产品条件判断：本轮有普通life/F组合的可重放表达能力增量及
独立安全核控制；没有新增隐式Box/RC/锁，没有Rust同语义运行、RSS、编译成本
或维护成本比较，产品级净突破尚未建立。B0继续Doing，完整语言/ZUI未终验。
