# Codex 接续施工入口

这是draft22的Rust源码至LLVM/native工程。语言规范与schema/raw wire **17**保持；当前已实现普通类型参数透明别名、有限Copy枚举CT、拥有ByteBuffer的Package/保存闭包及实际多模块Native协议。101/107研究分别为Inconclusive并净回退，108为NoGo并净回退；109完成普通库与绑定的独立两因素研究，精确NS实现限定录用；全部不利结果、近门和同二进制波动保留，未证明稳健普适Rust优势。最终身份以主树工单及实际收据为准。不要把研究证据Pass读成性能准入，也不要从下方历史例子推导当前语言边界。正式B0仍Doing，完整语言NotSatisfied，产品NotRun。

## 恢复与定位

源码ZIP可以直接构建。接续原历史时，从`PACKAGE.json`的`companions.history.file`取得实际bundle名：

```sh
git init --initial-branch=restore-unborn zil-checkout
git -C zil-checkout fetch HISTORY_BUNDLE_PATH "+refs/*:refs/*"
git -C zil-checkout checkout main
cd zil-checkout
node tools/handoff.mjs --check
```

将`HISTORY_BUNDLE_PATH`换成实际附件路径。handoff的`authority.main_tree_now`定位当前主树；历史绝对路径只说明来源。没有Git可以构建，但不能把缺失历史记为交接Pass，不要重新初始化Git冒充原审核链。源码、历史和可选审计附件的恢复说明见[DELIVERY](DELIVERY.md)。

接着读主树`AGENTS.md`、`_zil_design.md`、`_zil_goal.md`、[IMPLEMENTATION-STATUS](IMPLEMENTATION-STATUS.md)、[RESOURCE-HANDOFF](RESOURCE-HANDOFF.md)和派定工单。规范及具名附件定义语义，主树.task记录派工与状态，Git保存实现与审核，.agent保存持续决策。完整聊天不是冷启动前置。

## 先定位本轮结果

先读`evidence/REPORT.md`、`evidence/final-summary.json`、[终态差距](DRAFT22-TERMINAL-GAPS.md)、
[Rust四目标](RUST-GOALS.md)和[Roam审计](DRAFT22-ROAM-AUDIT.md)。101–110每项都区分
作者实现、非作者审查、生产取舍及完整正式验收。性能只属于其冻结源码、程序和
工具链；后来的冷恢复不重采性能，也不能把旧样本改标当前HEAD。

```sh
cargo build --offline --locked -p zil-cli
cargo test --offline --locked -p zil-driver --test impl103_generic_aliases --test impl105_ct_enum
```

根README提供泛型别名、CT枚举、拥有字节和平台消费者的实际入口。完整包、错误清理等
旧例仍是回归，下面保留原使用说明。

## 保留的完整包主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/package_ui/main.zil -o out/package-ui
printf '++r-q' | ./out/package-ui
cargo test --offline --locked -p zil-driver --test impl060
```

四模块事件程序输出0、1、2、2、1五帧、`#5`及清理标记`MSU`。同目录另有生命周期失败矩阵和两个真实fd清理程序；三个入口都有独立Rust对照，18个输入×四个原生版本共72次行为对照。源码与输入表见[README](../examples/mainline/package_ui/README.md)。

## 保留的同步错误主线

本轮环境为Linux x86_64、Rust 1.99.1/2024 edition、外部clang 23.1.5，实际工具身份由收据绑定。将工具放入PATH，或以`ZIL_CLANG`指定clang；动态库与系统链接器须可用。编译器workspace只依赖标准库和本地crates；Roam在独立workspace内按自己的锁文件准备依赖。Wine兼容执行不等于原生Windows/驱动验收。

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/error_cleanup/main.zil -o out/error-cleanup
printf 2 | ./out/error-cleanup
printf a | ./out/error-cleanup
cargo test --offline --locked -p zil-driver --test impl056
```

入口为四模块`examples/mainline/error_cleanup/main.zil`，独立Rust对照同目录`main.rs`。`2`输出`CBA`并退出1，`a`输出`vXBA`并退出0；stdout没有末尾换行，stderr为空。15个输入完整表在[样例README](../examples/mainline/error_cleanup/README.md)，ZIL/Rust O0/O2共60次行为对照，精确身份见[056收据](../evidence/impl/056/acceptance.json)。

有限Copy E!T在唯一匹配分支注入，?只传播同一E；普通main成功返回i32或0，错误退出1且不自动打印。LLVM内联双payload `{tag,E,T}`，并非紧凑union内存结论；错误tag投影为真实trap。待提交拥有参数/callee遇后续错误仍有局部载体负责清理。Copy字段求值失败不会构成完整资源，这不代表一般拥有聚合部分初始化。普通Return也可返回错误tag，scope边的Error标签只对应真正传播的错误值。

## 保留的资源闭包程序

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/resource_closures/main.zil -o out/resource-closures
printf D | ./out/resource-closures
printf U | ./out/resource-closures
printf 2 | ./out/resource-closures
cargo test --offline --locked -p zil-driver --test impl051 --test impl053
```

入口是三模块`examples/mainline/resource_closures/main.zil`，独立Rust对照为同目录`main.rs`，验收入口为`crates/zil-driver/tests/impl053.rs`。程序读取第一个输入字节，以下输出均无末尾换行，stderr为空。`E`和`0`/`1`/`2`的非零退出码是预期应用结果。

| 输入 | 覆盖路径 | stdout | 退出码 |
|---|---|---|---:|
| `D` | Desc捕获每次重读；通过helper写外部目标 | `C` | 0 |
| `S` | 拥有Fn共享调用两次 | `A` | 0 |
| `U` | 两个拥有捕获从未调用，按捕获顺序清理 | `AB` | 0 |
| `O` | FnOnce转交一个字段；callee清理后清剩余字段 | `CB` | 0 |
| `M` | 拥有FnMut通过helper修改并重复调用 | `C` | 0 |
| `E` | 拥有环境创建后提前返回 | `A` | 7 |
| `T` | 环境转交给新局部后调用 | `A` | 0 |
| `L` | 循环continue和break清理环境 | `AB` | 0 |
| `0` | outer构造完成后返回Outcome.Failed | `A` | 1 |
| `1` | 拥有环境构造完成后返回Outcome.Failed | `BA` | 2 |
| `2` | tail构造完成后返回Outcome.Failed | `CBA` | 3 |
| `3` | 完成调用并返回Outcome.Ok | `CBA` | 0 |

053历史收据记录12输入×ZIL/Rust O0/O2，共48次原生执行。Outcome是普通用户枚举，四条路径验证正常Return清理已完成资源；该样例本身不使用新E!T/后缀?，也不证明panic unwind。精确受测提交见[evidence/impl/053/acceptance.json](../evidence/impl/053/acceptance.json)。

## 保留已有程序回归

```sh
./target/debug/zil build examples/mainline/resources/main.zil -o out/resources
printf x | ./out/resources
printf '!' | ./out/resources
./target/debug/zil build examples/mainline/resources/fd.zil -o out/resource-fd
./out/resource-fd < /dev/null
./target/debug/zil build examples/mainline/stream/main.zil -o out/stream
printf '10 20 -3\n' | ./out/stream
```

资源程序分别为`EICFG|HA`/exit0、`EICA`/exit2，fd程序为`F`/exit0，均无末尾换行。fd例实际关闭成功取得的描述符，并与Rust OwnedFd对照。stream为`3 0 45`加换行、exit0；EOF/读错区分和最终flush失败仍不在现有外部协议内。这些使用真正main入口与显式C封装。相关限定入口为`impl047`、`impl049`、`mainline_stream`及CLI的`stream_io`；根据实际改动选择回归，root统一正式门。

## 当前能力与边界

- 有限完整 package 已作为普通非 Copy Value 贯通：直接 manages 字段拥有已有资源，其他字段为有限 Copy；字段准备临时失败时逆序清理，完整包按声明字段序清理，整体移动/参数/返回和字段 Desc 均有真实消费者。未公开部分完成的包对象，也未开放成品部分移动、替换、嵌套包或包内闭包保存。
- 资源payload限有限Copy。普通资源参数/别名/默认捕获为Desc，复制描述不复制责任；`move` Managed或正文`give`在创建时转交。Desc捕获目标在每次调用时读取；对外部Desc目标写入不自动要求FnMut。
- 拥有捕获只读为Fn，写入拥有目标为FnMut，移出责任为FnOnce。直接/递归/跨模块helper的Desc写效果参与判断，Copy字段快照的局部修改不会使原环境误判为FnMut。FnMut需可写闭包绑定；拥有环境不可复制，转交后旧环境不可使用。
- Analyze与Verify分别重建来源、责任和效果。原所有者give后仍活跃的旧Desc拒绝；Managed SSA从生成即持责。LLVM真实live flag控制条件清理，Drop进入前先撤原激活。局部逆成功初始化顺序清理，环境字段按捕获首出现顺序清理；Desc无noalias，无隐式Box/RC。
- 资源嵌套/替换、一般拥有聚合及部分移动、逃逸闭包、嵌套拥有环境捕获、资源型闭包显式参数/返回、Desc返回或聚合保存/逃逸、Address<Resource>尚未实现。现有闭包环境作为捕获项的一般组合仍未开放。普通资源和无关Copy闭包同函数共存已经支持。
- 有限词法storage scope、Copy错误联合和同步传播已贯通；拥有错误载荷、一般拥有聚合部分初始化、panic unwind、取消、异步frame和并发仍未实现。scope不限制所有SSA跨块结果，后者仍由全函数责任守恒核验；checked trap仍为进程abort。
- 一层共享/独占地址与重借、默认闭包更新外部Value、公共有限泛型/别名、函数指针、纯Copy编译期计算及标量C ABI继续可用。地址保守持有整对象根，没有不交字段精度/嵌套地址；readonly move &mut仍保守FnMut。foreign行为和unsafe外部合同仍可信。
- 问号全场景方案、逐项move、`@`和hunt/seek/zst/zis命名仍为候选。通用gen/open/lease不恢复。类型、缓存、属性、wire和哈希不制造Verified权限。

## 下一段施工

以已实现的完整 package、字段准备失败和有限错误/清理为起点，按[RESOURCE-HANDOFF](RESOURCE-HANDOFF.md)先贯通非逃逸高阶 helper，再推进可保存环境、一般拥有组合及可复用库。先固定真实责任与退出合同，再同批实现前端CFG、独立核验和机器路径；不要重派已完成的本地资源闭包、完整包或有限错误传播。逃逸、嵌套、unwind与取消依真实前置拆片。

059已实现冻结schema8真实TypedIR上的有限完整联合关系，mono/cut/wire与独立raw枚举oracle逐点核对；43函数、903次正式完整核对，相关分支展示精度机会。当前切割/等式代换无稳定整体成本优势，暂不入核；四个独立条件的BDD节点限额与另一oracle限额失败保留。切割仅在单事件方程内，全函数仍为联合固定点；不等于跨CFG区域摘要、无界动态代次或一般精确语言语义。按[059入口](research/059/README.md)从历史bundle恢复6ac依赖和c0c1dc算法，不能在当前schema10上冒充旧冻结结果。

055先前的may-liveness/带标记图保持没有实现联合状态。19源3192样本未建立优于块基线的完整成本收益；该候选未进入生产Verify。测量身份、全部收益与退化见[055研究入口](research/055/README.md)；050模型结果和046长链收益/小例退化继续按原身份保留。

一单一branch/worktree及独立target/output；作者限定测试，非作者审精确提交，root集成正式门。沿用现有流程，不增加审批层。最近反例与修复分别保持失败/通过的原始身份。Rust四目标按[RUST-GOALS](RUST-GOALS.md)逐轮核对；行为一致和模型加速不能替代运行、内存、维护及完整编译成本对照。

## 资源闭包质量探索与交付

现有`zil qc`保留session/scalar-native，并增加Rust `resource-closure-native` profile。它实际生成ZIL、编译并执行，用独立有限生命周期模型计算预期，复用hunt/issue队列及同性质重放/缩减/修复验证。054限定4项测试及无注入8 cases/32 actions通过，未发现真实生产缺陷。`closure-omit-call`是人工控制；051 Copy快照误拒来自手工边界测试。

```sh
cargo build --offline --locked --manifest-path tools/roam-rs/Cargo.toml
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam hunt --profile=resource-closure-native --queue=out/qc/issues --out=out/runs/resource-closures-001 --cases=8 --seed=53 --steps=4
```

`--out`每次使用新目录。长期探索与独立修复沿用[ROBUSTNESS](ROBUSTNESS.md)和[Roam README](../tools/roam-rs/README.md)；该profile是有限顺序生命周期/O0检查，不能替代053的O0/O2对照或一般资源协议证明。

root在干净提交上运行正式门：

```sh
python3 tools/validate-draft.py --publish-summary
```

实际受测hash与数字只读[evidence/final-summary.json](../evidence/final-summary.json)和[REPORT](../evidence/REPORT.md)。空运行、缺fixture、Unsupported、预算耗尽、超时、工具失败不算Pass。raw写唯一out/runs；主包只带源码与小收据，bin/IR原件按PACKAGE.json选择进入审计附件。历史bundle保留精确实现和审核。交付ZIP还须解压后构建、运行主线程序并恢复bundle/handoff，结果记录在包外交付报告；仓库受测提交与仅追加收据/文档的打包提交分别记录。
