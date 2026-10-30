# 218／220／221 联合源的独立审核

**有限范围裁决：218 Pass、220 Pass、221 Pass。** 三份 JSON 均使用 `reviewer=/root/language_parallel`、`implementation_commit=69931f83c11f8127d0d4aebe4e2bd6fe2aaeffbf`。审核者只实现过独立的 219，未参与这三个工厂组件生产实现。所有受测源码来自只读、clean 的最终组合树；没有修改生产、规范、原生 oracle 或默认限额。

最终源码 tree 为 `cc9f3580cb2dcce2b51f037a37acb87b1d8365b2`。独立新 target 构建 CLI 的 SHA256 为 `ad5154ba0dffacd6e96f33f55d2427719598afc5e9aad1f95bac0335bc6239e7`。原构建清单含 200 份 Cargo／crate 源输入，前后摘要相同；清单 glob 之外的 `tools/llvm/version.txt` 另按 clean Git 树核查，值 23.1.4、与初查提交同一 blob，没有冒称该补查是单独的构建前快照。

## 结论按组件区分

| 工单 | 实际审核范围 | 裁决 |
|---|---|---|
| 218 | 未调用定义约束、单直接自含 F、完整实例身份、新 opaque 返回身份、Fn/FnMut/FnOnce 声明模式沿实际局部／值／捕获位置传递，以及条件转交修复 | Pass |
| 220 | 独立 Analyze 重建真实参数／结果签名、递归捕获自含性、伴随参数完整形状、返回合同、实际拥有值责任与清理 | Pass |
| 221 | 独立 raw／Wire Verify、返回合同／来源／布局／责任，内联机器表示，实际调用／销毁／错误退出及固定消费者各实际分配失败 | Pass |

三者共同完成的是 `_zil_design.md` §9.4.1 的有限普通拥有工厂。不是完整 1.4、全部 ZUI、异步／取消／并发或产品门结论。一般直接返回 F、多 F、隐藏 State 输入、动态擦除与保留借入环境等仍保留各自边界。

## 一处真实误拒已由原字节关闭

在旧组合 `2212505a667508dc9c7565cbab674c8e59700457` 上，返回闭包内的 `let g=if flag{f}else{f};return g()` 越过 Expand，却在 Types 被错误推为 Fn 并因公开 FnOnce 的精确 kind 要求而拒绝。直接 alias、把 if 放在工厂外层、分支直接调用均正常；二次 g() 反例被正确拒绝。这是实际源级误拒，没有发现安全反例被放行。

修复提交 `4d331dd6b321685ea575a132c05dfb03aa7758fc` 只对带工厂声明模式的拥有 Closure 条件结果做实际捕获转交标记；两臂先完成原推导和统一，再支付 scope 遍历与名字查找费用，保持词法 lookup，不按 ClosureId 赋予权限。借入与无该声明来源的旧路径不进入新扫描。

最终组合上，**五份原始源码／oracle 全部原字节通过复验**：四个正例 Verify Success，二次调用仍为 Expand SafeReject。原误拒正例的 O0／O2 各一次普通 LLVM API 产物均真实执行，退出 0、stdout／stderr 为空且无后端回退。旧失败和初次审核报告完整保留。

## 独立新输入与实际值对照

父审核者另外冻结并执行七份有效源码：三个正例覆盖同型不同位置、F 位于非首参数的跨模块 grounded 转交、未调用销毁与实际调用；四个近邻分别检查未调用定义的二次消费、转交后复用、opaque kind 升级和外借 grandchild。三个正例形成六个 O0／O2 构建、**八次真实执行**，stdout／退出码全部匹配原预期。加上上节修复正例的两次，此轮新设计原生输入共十次运行；没有加入作者的 42／100 次或初查次数。

一个最初审查夹具把 unsafe 写成表达式，实际在 Parse 拒绝；原源和失败保留。另行冻结普通 read() 函数中的合法 unsafe return，机器 stdout／退出码 oracle 不变。此夹具修正未登记为编译器缺陷。

实际值核使用真实源派生 TypedIR：两个捕获有相同 ClosureId，但 ValueId 不同。保持原槽数、类型和布局，仅重复第一个 ValueId 时，Analyze 和 raw／Wire Verify 都拒绝；Verify 为 `E-VERIFY-RESOURCE`，排除了失效 plan identity 与 ResourceLimit。只交换两个独立 ValueId 是另一个合法程序，重新 Analyze 后两条 Verify 路径均接受。最终提交上实际执行 **3 次 Analyze＋3 次 raw Verify＋3 次 Wire Verify**，没有把同型等同于同一运行位置。

## 预算、旧输入与既有故障 oracle 的新重放

三个旧输入在最终提交上重新执行 N／N−1／fresh N。源哈希、Core／Typed 文本与冻结旧基线一致，Analyze／raw／Wire 的额度和指纹也与原基线完全一致。旧编译器未在本次重新构建；比较的是已留存的实际旧基线收据。

| 固定输入 | Expand 工作 N | Types 工作 N | Types locals / functions | Analyze N | raw / Wire Verify N |
|---|---:|---:|---|---:|---:|
| helper | 586 | 421 | 6 / 4 | 1464 | 1908 |
| State | 829 | 1441 | 8 / 4 | 4701 | 7043 |
| 普通 opaque | 3 | 462 | 7 / 3 | 3960 | 5479 |

原 owning、forwarding 两组前端阈值保持 724／770 和 1438／1556。修复条件转交的新输入为 Expand 818、Types 978、locals 14、functions 5，N−1 按对应阶段 ResourceLimit 拒绝，fresh N 与默认输出相同。源派生新工厂 raw／Wire Verify 均为 16559，并保留 N−1／fresh 相同指纹控制。

另外，本审核者在最终 699 源上实际重新运行了已提交的 221 原 oracle：**10 个测试组通过，42 次原生重放（20 次普通、22 次观察器），6 次实际分配失败，所有终态 live 为 0**。它覆盖参数准备失败、回调 Store、Copy／slice 地址签名、Copy 错误结果及合法 lending child。这里清楚标为“非作者重新执行既有 oracle”，原作者的旧 42 次不在本次计数中，219 的 100 次也没有计入工厂独审。

## 收据和边界

`REVIEW-218.json`、`REVIEW-220.json`、`REVIEW-221.json` 是分别按工单 scope 填写的正式有限审核结果；共同使用最终 source SHA。原始包和逐文件 manifest 包含新构建、冻结输入、全部源码／阶段／Wire／计划／LLVM／原生可执行文件、命令及 stdout／stderr，以及旧误拒和夹具失败；未复制编译器二进制或外部工具链。

审核未运行正式整仓，root 的本地全仓磁盘失败与 GitHub CI 由集成方单独留证和裁决。此次私有复验没有 ENOSPC；在 root 通报磁盘压力时暂停了新工作，恢复后使用 strip=symbols，并在所有对应子进程结束后清理自有可重建缓存，保留了所有证据及两个已分发 CLI。

四目标判断保持有限：真实权限与清理、普通公共 API 组合、旧预算保持有实证；没有同语义 Rust 运行时间／RSS／编译性能优势、真实框架维护成本或产品级净突破结论。完整语言 NotSatisfied、B0 Doing、产品与原生 Windows NotRun。
