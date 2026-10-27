# ZIL — 语言主线 166–169（2026-10-27）

本轮语言入口见 [166–169 实施报告](docs/LANGUAGE-169.md)：有限 Package 错误载荷、固定拥有型数组、具体闭包转发/再捕获及稳定替换槽。当前内部 schema/raw wire 为 **19**；完整语言 NotSatisfied、产品 NotRun、B0 Doing。源码包按 Git 跟踪内容导出，执行产物和证据在主包之外；见 [交付与恢复](docs/DELIVERY.md)。实际验收以本轮报告与精确提交的收据为准。

以下保留各轮历史介绍与原受测数字，旧文中的“当前”“本轮”和 schema 版本属于其对应历史检查点，不代表 166–169 的能力或验收身份。

## draft23 施工恢复检查点（2026-10-26，历史）

120字段借用精度、121跨模块callable名义类型、122纯CT与运行时析构组合已独审合入。合并后整个Rust workspace **1195项通过、0失败、0忽略**；三项能力合成的原生程序在O0/O2均输出T42，最近的同字段冲突仍拒绝。见[恢复与语言主线报告](docs/DRAFT23-RECOVERY.md)及[实际集成收据](evidence/recovery-20261026/integration.json)。

当前是可恢复开发检查点，**不是完整正式发布验收**。本轮未重跑全部35项正式门，Win64/Wine未重建；完整语言NotSatisfied、产品NotRun、B0 Doing保持。Roam仅以已有profile低优先级伴随运行，没有新增平台工作。源码/历史增量的恢复方式见[交付说明](docs/DELIVERY.md)。

## 运行本轮三项能力的组合

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/recovery_combination/main.zil --entry value --opt O0 -o out/recovery-combination
./out/recovery-combination
cargo test --offline --locked -p zil-driver --test impl120_field_backing --test impl120_field_backing_native --test impl121_callable_nominals --test impl122_ct_declarations
```

预期原生输出`T42`及换行；组合与边界见[样例说明](examples/mainline/recovery_combination/README.md)。

## 已保存draft22基线（以下为历史范围）

当前版本沿用 schema/raw wire **17**。普通类型参数的透明别名、有限 Copy 枚举的 CT 执行与普通重化重检已进入主线；二者已在多模块程序中组合执行到完整 64 位结果。拥有 ByteBuffer 的 Package 与可保存闭包继续保留实际机器清理，没有增加隐式 RC、堆盒、世代或宿主 Rust 借用检查。

<!-- draft22-formal-begin -->
正式RunId `20261025T140238Z-8db4b7c0`，受测提交 `9a0f37ead42c700bbd29db8be67a6e058ed1297e`：**35/35必要门、1169项工作区Rust测试、39项独立Roam测试通过**。首轮34/35失败及110旧CT契约迁移保留。精确身份见[正式收据](evidence/final-summary.json)和[验收报告](evidence/REPORT.md)。完整语言 **NotSatisfied**、产品 **NotRun**、B0 **Doing** 保持。
<!-- draft22-formal-end -->

Roam 的拥有字节 **/2** 合同覆盖局部、Package、FnMut、FnOnce 四类载体，使用独立 Rust 模型、C 参考和 ZIL/C 各 O0/O2，并核对真实分配身份、失败保持和释放顺序。自然运行的 48 个唯一程序仍是 **0 findings**；受控注入与独审发现不能转记为 Roam 自然发现。范围见[本轮 Roam 审计](docs/DRAFT22-ROAM-AUDIT.md)。

101 参数事实图和107私有构造身份均已完成实现、固定完整成本、全部端点独立复算及净回退。主要完整进程比分别为 **0.708151** 和 **0.747634**；全矩阵尚不能满足既定1%回退界，均为 **Inconclusive**，未进入生产默认路径。证明、原件与候选历史保留，见[本轮架构研究](docs/research/DRAFT22-ARCHITECTURE.md)。

104 已实际测量同合同 Rust 编译、运行和内存成本，三个4MiB运行例未达原0.9线；108单独外部绑定仍有一例未过线，保持NoGo。109完成450行为、1920时间和300目标RSS观察，普通字节映射库与精确Linux x86_64绑定组合在十二个有限单元上通过原数值门并限定录用。三个4MiB大输入仍比Rust慢，三个中位数、四个p95不利及同二进制波动完整保留；不声称稳定普适胜出。旧结果不改写。终态义务与四目标见[差距矩阵](docs/DRAFT22-TERMINAL-GAPS.md)和[Rust 对照](docs/RUST-GOALS.md)。

## 运行本轮语言组合

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/generic_alias/main.zil -o out/alias-demo
./target/debug/zil build examples/ct_enum/main.zil -o out/ct-enum-demo
cargo test --offline --locked -p zil-driver --test impl103_generic_aliases --test impl105_ct_enum
```

[泛型别名四模块程序](examples/generic_alias/README.md)组合真实泛型记录、枚举、普通函数和编译期表；[CT枚举三模块程序](examples/ct_enum/README.md)在编译期构造嵌套计划，再消费实际运行时字节。空输入、完整字节值及错误路径均有保留证据。带 bound/const/lifetime 的别名和一般拥有 CT 值仍不支持。

## 运行保存动态字节状态的消费者

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/byte_buffer_owned/main.zil -o out/owned-bytes
./out/owned-bytes < input.bin > output.bin
cargo test --offline --locked -p zil-driver --test impl099_owned_bytes --test impl099_owned_bytes_native
```

非零输入追加当前批次，输出累计字节值与批长度的函数；零输入替换拥有字段并释放旧批次。精确行为与独立 Rust 对照见[样例说明](examples/mainline/byte_buffer_owned/README.md)。[字节合同](docs/BYTE-STORAGE-HANDOFF.md)、[当前施工状态](docs/IMPLEMENTATION-STATUS.md)、[设计反馈](docs/DRAFT21-DESIGN-FEEDBACK.md)和[Rust 四目标](docs/RUST-GOALS.md)分别说明支持边界。七件交付及恢复入口见[DELIVERY](docs/DELIVERY.md)。

## 运行动态字节消费者

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/byte_buffer/main.zil -o out/dynamic-bytes
./out/dynamic-bytes < input.bin > output.bin
cargo test --offline --locked -p zil-driver --test impl096_byte_buffer_native
```

消费者保留NUL和非ASCII字节，把ASCII小写转大写；动态存储不是完整Unicode字符串库。可运行工厂/give、泛型窗口和失败路径见[样例说明](examples/mainline/byte_buffer/README.md)。七件交付、全部可见Git历史和冷恢复说明见[DELIVERY](docs/DELIVERY.md)。

## 先运行纯 ZIL Win64 首窗

构建及操作按 [Win32 样例说明](examples/mainline/win32/README.md)。`tools/platform/087/imports.py`生成显式导入库，`zil build --target win64 --opt O0|O2`生成真实 PE，`verify.py`记录真实交互、截图、退出码和产物摘要。交叉目标不由 `zil run` 隐式启动。环境依赖的具体版本和恢复边界见[ENVIRONMENT](tools/platform/087/ENVIRONMENT.md)。

七件交付包含源码、全部可见 Git 历史、审计原件、冷恢复、机器/文字报告及 SHA256；入口见[DELIVERY](docs/DELIVERY.md)。旧字节流与拥有组件程序继续作为回归。

## 先运行字节流主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/byte_io/main.zil -o out/byte-stream
printf 'hello, Zil!\n' | ./out/byte-stream
cargo test --offline --locked -p zil-driver --test impl083_bytes_native
```

输出`HELLO, ZIL!`及换行；ASCII小写转大写，其他字节原样通行。完整条件及Linux x86_64的C ABI限制见样例说明。七件交付、全Git历史及独立冷恢复入口见[DELIVERY](docs/DELIVERY.md)。

## 继承的组件拥有环境与历史研究

继承的draft16 077–079把完整有限Package移入真正可保存的闭包环境：工厂可返回拥有整个Window的Fn/FnMut/FnOnce，调用者跨多轮事件借用、更新或消费环境。唯一词法view直接引用环境数据及持久字段责任标志，Bind/End不复制或重置组件。FnMut所有普通及错误返回必须恢复完整；FnOnce部分消费后由caller清理剩余字段，Trap仍abort。前端/Analyze、独立raw/wire Verify及机器后端分别接受非作者审核。

[owned_component](examples/mainline/owned_component/README.md)提供五模块事件环、三模块生命周期和真实fd入口，20个固定输入在ZIL/Rust各O0/O2上完成80次行为对照；独审另核异形双Package和混合捕获布局。这些是有限同步组件行为，平台窗口、GPU、字符串/动态容器和OS回调取消排空仍未完成。

[076研究](docs/research/076/RESULTS.md)补齐dense-direct强显式基线和两种整态共享方案，526个Complete函数查询全部关系逐字相等，10 ResourceLimit和8 OracleResourceLimit保持原状态。四guard完整候选dense-direct4.268ms、local-direct4.523ms：局部域及整态共享相对强基线尚无稳定净收益，旧071的大幅改善主要来自直接转移。原504次采样与负结果保留，生产NoGo；下一事件支持、可证分解及纵向事实共享见[架构分析](docs/research/DRAFT16-ARCHITECTURE.md)。

继承的draft15 072–074把直接Managed字段取走、补入/替换及有限Copy字段写入接通源码至独立Verify与native。新资源先准备成功，再清理旧字段并提交；部分形状留在局部，完整参数/返回/SSA边界仍须补齐。精确字段Desc阻止旧实例被悄悄替换，兄弟字段可继续使用。[update_ui](examples/mainline/update_ui/README.md) 的20组输入×ZIL/Rust各O0/O2覆盖80次实际执行，包括真实Linux描述符关闭。完整值ABI不变，局部字段flag不进入跨函数布局。

[071历史研究](docs/research/071/RESULTS.md)及其[当时架构说明](docs/research/DRAFT15-ARCHITECTURE.md)保留冻结schema10与原始测量身份。076补充强显式控制后收紧了收益归因；不将旧数直接作为局部图切割的独立收益或整体编译提速。

继承的069–070 接通有限 `-> impl Fn/FnMut/FnOnce` 工厂：真实自包含 Copy/Managed 环境按值返回，调用者跨多轮同步事件保存、移动、调用并准确释放。公开返回合同由独立 Verify 核验，raw 参数和捕获均限制自包含边界。五模块 [retained_ui](examples/mainline/retained_ui/README.md) 对20个输入完成ZIL/Rust O0/O2共80次行为对照。当前不是OS回调注册、动态callback容器或完整ZUI。

[067 物理/数学启发的完整关系研究](docs/research/067/RESULTS.md)实现并检验token位置编码：258个Complete函数查询与独立oracle全量一致，但主要样例更慢，四guard的fused总成本比为1.802。候选保持NoGo；关联位更少并不等于因子求解更便宜。[068纵向通道](docs/research/068/results.md)删除重复解析、Typed中转复制和输入身份构造，288次完整请求保持语义一致，但未证明普遍稳定的时间收益。路线、Rust/Zig/Polonius与物理文献的具体映射见[架构研究](docs/research/DRAFT14-ARCHITECTURE.md)。


上一轮 064–066 接通有限、同步、非逃逸的具体环境 helper：`F: Fn/FnMut/FnOnce` 在实际环境已知后生成独立函数，`&F`/`&mut F` 保留原位置，按值 F 转交责任。Hold/Call/保守写上界/Cleanup/有限轨迹经独立 Verify 后导出，公共 helper 调用点不读取 helper 私有写效果摘要。[五模块 UI 和四模块生命周期程序](examples/mainline/helper_ui/README.md)有 25 个输入、ZIL/Rust O0/O2 共 100 次行为对照，包含实际调用模式适配、零次销毁、第一/第二次错误与晚实参失败。该helper仍为非逃逸参数；新的自包含工厂返回见上文，图形平台仍未实现。

[063 编译成本与联合关系研究](docs/research/063/README.md)把 BDD 临时节点回收、关系变量排序、分块/融合提前消元分别受控比较，保持独立 raw oracle 和完整关系核对；同时测实际 driver 冷/复用/改动成本和 Rust 分阶段成本。结果按其冻结源码、原始样本和完整成本解释，不将研究收益外推为 Rust 产品胜出。由该定位发现的未请求 Debug dump 提前格式化已经在生产 driver 改为按需计算；必要快照指纹、Verify、取消和输出保护仍执行。

上一轮 060–062 接通有限完整 `package`：多个 `manages` 资源和 Copy 字段组成普通非 Copy Value，可整体移动、跨模块传参/返回、借用实际字段及按声明顺序清理。准备失败仍由真实 owning 临时负责逆序清理，完整包在全部准备成功后原子提交。三个[实际程序](examples/mainline/package_ui/README.md)把资源组合用于四模块有界 UI 事件环、逐准备点失败和两个真实文件描述符；18 个输入在 ZIL/Rust 各 O0/O2 上作 72 次行为对照。平台窗口、GPU 和一般保存/逃逸的回调尚未实现。

此前新增 [同步错误四模块程序](examples/mainline/error_cleanup/README.md)：有限 Copy `E!T`、后缀 `?`、唯一载荷类型注入及显式清理作用域贯通源码至 LLVM/native。15 个输入在 ZIL/Rust O0/O2 上核对 **60 次原生执行**，覆盖成功、逐创建点失败、待提交 give 参数、FnOnce callee、未完成构造、闭包正文错误和循环退出。旧资源闭包、实际文件描述符和 stream 程序保留回归。正式必要门和测试计数只以 [本轮验收报告](evidence/REPORT.md) 与 [机器收据](evidence/final-summary.json) 的精确受测提交为准。

[059 联合关系研究](docs/research/059/RESULTS.md) 已在冻结的 schema8 真实 TypedIR 上实现有界完整联合状态，比较单体方程、提前消元切割与等式代换；独立 raw oracle 逐点完整核对，保留责任、Desc、分支、错误及全部出口。8 个完成入口、43 个函数，224 次正式进程运行中有 903 次 shadow 函数全关系核对。相关分支实例显示比现有生产合流更精确的可能性，但当前两个候选没有稳定整体成本优势，暂不入核。四个独立条件即触及 BDD 节点限额的反例和另一 oracle 限额样本均保留；有限抽象、旧版依赖、完整成本及不支持边界见固定结果。生产接受集合与 Verify 不变。

[055 真实 TypedIR 研究](docs/research/055/README.md) 是先前的 may-liveness 与带标记事件图研究。19源、3192样本的配对cut/block总成本中位比均大于1（1.007–1.147，多数样例波动跨1）；它本身没有实现联合状态关系，不能把059的新能力倒记到旧结果。冻结输入、预算、所有收益及退化只读 [055 结果](docs/research/055/RESULTS.md)。

[050 图分解专项](docs/research/050/README.md) 已交付独立 Rust 原型、完整枚举 oracle、同精度单体基线与冻结实测。窄链模型约快 217.7 倍，密图约慢 5.34 倍，小闭包模型也更慢；这些是研究模型结果，尚未接入生产编译器，更不是相对 Rust 的收益。054 同时把有限资源闭包接入原生 Roam 的既有 hunt/issue 流程。

[046 依赖前沿优化](docs/PROOF-CUT-PERFORMANCE.md) 从附件吸收“只传播相关边界”的思路：反向256函数链完整 check 从285.842ms降至176.955ms，Analyze/Verify阶段约45倍改善；小程序完整 check 仍观察到9–22%退化。测量绑定独立046候选与旧ZIL基线，不是融合后draft09的性能门，也不是相对Rust的胜出。

**完整语言仍为 NotSatisfied，产品门 NotRun，正式 B0 仍 Doing。** 这是可运行的语言子集；单个切片通过、parser 接受或格式版本更新都不等于正式工单完成。精确能力见 [施工状态](docs/IMPLEMENTATION-STATUS.md)。

## 先运行保存整个组件的主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/owned_component/main.zil -o out/owned-component
printf '+r-q' | ./out/owned-component
cargo test --offline --locked -p zil-driver --test impl077_package_captures --test impl079_package_captures
```

输出 `0\n1\nS1\n0\n#4\nMSU`、退出0。Window由工厂返回的拥有FnMut环境持续持有；S是替换旧Surface，最终MSU是剩余组件字段按声明顺序清理。完整输入、生命周期与真实fd边界见样例和审核收据。首次准备独立Roam工具时仍须按其已有锁文件fetch第三方依赖；生产14个crate只用标准库及本项目路径依赖。

## 保留的组件字段更新主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/update_ui/main.zil -o out/update-ui
printf '+r-q' | ./out/update-ui
cargo test --offline --locked -p zil-driver --test impl072_packages --test impl074_packages
```

输出 `0\n1\nS1\n0\n#4\nMSU`、退出0。S是替换时清理的旧Surface，最终MSU为剩余字段的声明顺序清理；输入f在替换准备时返回错误，旧Surface仍在最终清理中。

## 保留的保存环境事件主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/retained_ui/main.zil -o out/retained-ui
printf 'l++-q' | ./out/retained-ui
cargo test --offline --locked -p zil-driver --test impl069_retained --test impl070_retained
```

输出 `F0\n1\n2\n1\nA`、退出0。F是工厂退出时的临时资源，A是跨事件保存的环境资源；较晚实参失败输入 `p` 输出 `BCDZ`、退出1。

## 保留的公共 helper 与 UI 资源主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/helper_ui/main.zil -o out/helper-ui
printf '++r-q' | ./out/helper-ui
cargo test --offline --locked -p zil-driver --test impl064 --test impl065_helpers
```

依次打印 `0`、`1`、`2`、`2`、`1` 五帧，再打印 `#5` 和清理标记 `MSU`，退出 0。未知事件经 `?` 退出也清理已暂存的整包；有界事件循环、字段 Desc 回调与真实 fd 的界限见样例 README。相同行为不是性能比较。

## 保留的同步错误主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/error_cleanup/main.zil -o out/error-cleanup
printf 2 | ./out/error-cleanup
printf a | ./out/error-cleanup
cargo test --offline --locked -p zil-driver --test impl056
```

`2` 输出 `CBA`、退出 1；`a` 输出 `vXBA`、退出 0。输出没有末尾换行，完整 15 输入表见样例 README。错误载荷与成功载荷目前都限有限 Copy；机器表示为内联 `{tag,E,T}`，包含两侧存储。没有从本例推导相对 Rust 的运行或内存性能优势。

## 保留的资源闭包主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/resource_closures/main.zil -o out/resource-closures
printf O | ./out/resource-closures
printf 2 | ./out/resource-closures
cargo test --offline --locked -p zil-driver --test impl051 --test impl053
```

`O` 输出 `CB`、退出 0：消费一个捕获资源后清理剩余字段。`2` 输出 `CBA`、退出 3：显式失败值返回前按责任顺序清理。输出没有末尾换行，完整 12 输入表见样例 README。

## 保留的普通资源主线

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/resources/main.zil -o out/resources
printf x | ./out/resources
printf '!' | ./out/resources
cargo test --offline --locked -p zil-driver --test impl049
```

第一条输入输出 `EICFG|HA`、退出0；第二条输出 `EICA`、退出2，均无末尾换行。`fd.zil` 另实际测试转交后的自动 close。源码、输出含义、Rust对照和外部合同见样例README。后续分包见 [资源交接](docs/RESOURCE-HANDOFF.md)。

## 构建并处理真实输入

实测环境为 Linux x86_64、Rust **1.99.1**、Rust **2024 edition**、外部 LLVM/clang **23.1.5**。`rust-toolchain.toml` 固定 Rust 版本；Rust 自带 LLVM 与 ZIL 外部后端用途不同。生产 14 个 workspace crates 只使用标准库和本项目路径依赖。

把 Rust 和 clang 放入 `PATH`，或用 `ZIL_CLANG` 指定 clang；外部 LLVM 共享库与系统链接器须可用。历史绝对路径不是新机器的配置要求，工具链使用身份由实际收据绑定。

```sh
cargo build --offline --locked -p zil-cli
./target/debug/zil version
./target/debug/zil check examples/mainline/stream/main.zil --json
mkdir -p out
./target/debug/zil build examples/mainline/stream/main.zil --output out/stream --emit-dir out/stream-stages
printf '10 20 -3\n' | ./out/stream
printf '1 broken 2 -4 +5\n' | ./out/stream
```

两次输出分别为 `3 0 45` 和 `4 1 23`，均退出 0。输出三列为 `accepted rejected total`：有效数依次交替经过 `x * 2`、`x + 11`，无效 token 只增加 rejected。输入按 ASCII 空白切分，接受绝对值至多 1000000 的有符号十进制整数；最多处理 1000 个 token，超过时退出 2 且不输出部分统计。空输入输出 `0 0 0`；`putchar` 当场报告写入失败时退出 3。这些限额属于应用协议，不是语言的通用输入限制。

`model.zil` 提供公共泛型 `Option`/`Result`、`Stats` 与 identity；`tokens.zil` 使用 `&mut` 更新私有解析状态；`main.zil` 的默认闭包更新外部 Stats，并调用 `transform.zil` 中的函数；`io.zil` 将私有 C 声明封装在显式 `unsafe` 内。解析、控制流、格式化和计算均由 ZIL 编译执行，C 只提供 getchar/putchar。此入口不依赖 `--entry` 测试打印包装。

```sh
cargo test --offline --locked -p zil-driver --test mainline_stream
# CLI 普通/JSON pipe 的集成回归。
cargo test --offline --locked -p zil-cli --test stream_io
```

### 保留的固定输入示例

[draft07 主应用](examples/mainline/app/README.md) 仍作为固定数据组合回归：编译期生成数据、泛型枚举和模式条件、函数指针、owned Copy FnMut、闭包副本独立及共享引用读取。其历史行为结果为 `475000738663`，普通 `main` 自检退出 0。

```sh
./target/debug/zil run examples/mainline/app/main.zil --entry result
./target/debug/zil run examples/impl042/closure.zil --entry answer
./target/debug/zil run examples/impl043/main.zil --entry result
```

后两条期望分别为 `422`、`5000000042`。`--entry` 由后端生成原生测试 wrapper，只用于观察完整返回值；它与 stream 程序的源语言 IO 模块是不同入口。历史行为结果不自动构成本轮新成绩。

## 当前能力与边界

| 范围 | 已贯通行为 | 当前边界 |
|---|---|---|
| 标量与控制流 | i32/i64/bool/Unit、绑定与赋值、递归、条件/循环、半开整数范围、短路、管道、match/if let/while let | checked 与显式 wrapping 算术；普通不可达源码的类型形状检查仍有 Unsupported 缺口 |
| 有限数据 | Copy tuple、名义 struct、定长 array、Unit/tuple 载荷 enum；构造、更新、投影读写、值传参/返回、接收者 | 无字符串、动态容器或一般拥有聚合；有限完整 package 单列如下；动态索引先做边界检查 |
| 完整拥有包 | 有限内联 package 组合直接 manages 资源与 Copy 字段；准备临时、原子提交、整体移动/传参/返回、字段 Desc、完整清理 | 无成品部分移动/替换、嵌套包、包内闭包、拥有错误载荷或自定义整包 Drop |
| 公共模块与泛型 | 公共普通函数/记录/枚举、有限泛型函数/记录/枚举、非泛型透明别名跨模块使用；显式或唯一约束推导 | 私有声明和字段受检查，别名保留定义身份；跨模块省略字段默认值、泛型别名、一般 bounds/trait 和泛型方法未完成 |
| 共享与独占地址 | 一层 `&T`/`&mut T`、投影、共享/独占重借、间接写入、直接函数参数与返回、独占地址转交 | 目标限有限 Copy 值；投影保守持有整对象根，无不交字段权限、嵌套地址或两阶段借用 |
| 局部闭包 | 默认共享/独占 Value 捕获、地址重借；Desc 捕获、拥有 Managed 环境；Fn/FnMut/FnOnce；转交、未调用及剩余字段清理 | 拥有环境不可复制；参数/结果限 Copy；自包含工厂返回单列；借用逃逸、嵌套环境和一般高阶容器未完成；只读 `move &mut` 仍保守 FnMut |
| 公共环境 helper | 一个静态具体环境参数；真实模板实例、Fn/FnMut/FnOnce 能力适配；Hold/Call/Cleanup 和有限调用轨迹公开核验 | 每路径最多两次，FnOnce 最多一次；无循环/递归/转发、环境返回/保存；callback 参数和结果限有限 Copy，FnMut helper 用 `&mut F` |
| 自包含工厂返回 | 普通工厂返回 `impl Fn/FnMut/FnOnce`；真实拥有环境跨函数和同步循环保存；公开cleanup和kind独立核验 | 工厂输入/捕获限Copy或Managed，回调签名限Copy；一种隐藏身份、精确kind，无工厂转发/递归、借用逃逸、OS保存回调或异构容器 |
| 函数值 | 无环境函数指针、参数/返回/聚合存储、间接调用；明确 fn 上下文的无捕获 lambda | 间接签名限有限纯 Copy；捕获环境和 foreign 函数不能擦除成普通 fn 指针 |
| 编译期 | 纯直接调用、递归、控制流及有限 Copy tuple/array/record；独立 CT 核验、有界 VM、结果重新进入普通链 | 不支持 CT 地址、enum、函数指针/闭包、foreign、Type/Syntax、反射或生成 |
| 资源 | 有限 Copy payload 的 resource；Desc、Managed give、工厂结果、隐藏 Drop；资源与闭包组合，正常返回/循环/临时环境清理 | 只开放无整体Drop包的直接字段取走/补入/替换；无嵌套拥有结构、一般拥有聚合、描述返回/逃逸或 Address<Resource>；拥有载荷、unwind/取消未完成 |
| 同步错误 | 有限 Copy `E!T`、唯一类型注入、`?` 同错误类型传播、词法存储作用域和本侧实际责任清理；普通 main 成功/错误退出 | 不隐式转换错误类型；两侧同型的隐式注入拒绝；无拥有错误载荷、通用部分初始化、unwind/取消；CT 错误联合执行未开放 |
| Native FFI / IO | 私有无正文 `extern "C" fn`；i32/i64 参数，i32/i64/Unit 返回；显式 unsafe 调用；getchar/putchar 封装 | 限已核 Linux x86_64 C ABI；无指针、varargs、bool/聚合签名、未知 ABI 或完整 IO 资源协议 |

[语法速读](docs/LANGUAGE-QUICKSTART.md) 提供既有源码写法；本轮新增边界以本页、[施工状态](docs/IMPLEMENTATION-STATUS.md) 与对应源码为准，完整语言规则仍只由规范定义。

### 地址、转交与闭包

`let q = p` 在 p 为独占地址时转交资格，原位置不能继续使用；直接调用和返回按来源合同重借、替换实际根。活子窗口暂停相交父资格，最后使用后才恢复。共享地址不能升级为独占地址，两个名字也不证明不相交。字段与数组投影仍使用真实地址，并保守追踪整个对象。

默认闭包直接写外部变量时保存真实独占借入；`var f = () => { ... }` 的调用修改原位置。owned Copy 的 `move` 环境则修改自己的副本，Copy 环境复制后状态独立。`move` 一个 `&mut` 会转交地址资格而非复制目标，含该地址的环境不能复制；目前即使只读该目标，也要求 FnMut 和可写闭包位置。地址嵌套、捕获地址变量本身的重绑定、部分重借循环及一般带环境逃逸仍有明确限制，不能把这一层地址能力等同完整资源模型。

### 公共类型、隐私和编译期值

文件即模块。`import a.b as alias` 解析到唯一 `a/b.zil` 或 `a/b/mod.zil`；目录入口显式给出，不自动合并文件。公共模板实例保留定义模块及其私有 helper 的绑定；不同模块同名、同布局类型仍不同。透明别名和同模块的两个导入名不产生新名义身份。未使用模板和别名仍检查。

公共记录可带私有字段，外部可以接收、传递整个值并使用公开操作；私有字段读取、写入、借入和源构造会拒绝。合法 CT 工厂结果重化是已有值复制，后续仍经类型、分析和独立核验；它没有授权用户在 CT 内构造或读取私有字段。跨模块省略字段默认值目前明确 Unsupported。

内存调用方用 `ProjectSources` / `compile_project` 明确提供模块图；`compile_source` 保持单文件。依赖或路径映射变化会保守失效相关阶段并再次 Verify，尚不是完整细粒度合同增量实现。

### 显式 unsafe 和 C 可信边界

foreign 声明只提供 ABI 和符号。普通 `Call`、普通函数引用、外层 unsafe 中创建的闭包或 CT 都不能替代调用处的显式 unsafe 区域；闭包正文中的 foreign 调用需要自己的 unsafe。独立 Verify 核签名及 CFG 的区域进入/退出和汇合，不接受一个未经检查的布尔标签作为授权。

C 实现遵守签名、不跨 ZIL 栈展开、不保留 ZIL 存储等外部约定属于可信边界，编译器没有静态证明 libc。当前 getchar 不能区分 EOF 与底层读取错误，putchar 之后、进程退出时的最终缓冲刷新错误也不在示例协议内。libc 的锁、缓冲和可能分配仍是真实成本；这不是完整标准 IO 库。

## 可信编译链与候选

正式链为 `Source → Syntax → Core → TypedCFG → CandidatePlan → 独立 verify → VerifiedModule → LIR → LLVM → native`。types、analyze、缓存和 raw wire 只提供候选；Verified 包装由核验函数私有构造，lower/backend 消费已核对象。CT 的独立白名单与有界 VM 不因普通 IR 增加操作而自动扩大权限。ZIL 安全核不转译为 Rust，也不调用宿主借用检查器作裁决。

历史 schema/wire 12 发布与当前 schema/wire 17 都不等于正式 B0 全形状 `schema.v1` 已完成；旧格式与历史收据按原版本识别，不能暗中复用旧缓存。规范仍为 **1.3/A**，`&T? = (&T)?`。用户定义 Option/Result 与有限错误传播可运行；`T?`、可空链与默认值运算仍未接通。018 前缀实验、局部 move、`@` 与逐项捕获写法仍为候选，本轮不改变它们。

下一阶段以完整/局部部分 package、本地资源闭包、有限保存环境及同步错误为基础，推进拥有组合、容器/字符串和可复用平台接口；先固定真实责任与退出合同，再同批对接前端、独立核验和机器路径。一般 trait、环境逃逸、dyn/opaque、动态容器、挂起/并发、SABI 和完整元编程仍未完成；按 [并行计划](docs/PARALLEL-PLAN.md) 的实际依赖推进。

## 常用命令与接续

| 目标 | 命令 |
|---|---|
| 选择检查阶段 | `zil check path.zil --stop-after parse\|core\|types\|analyze\|verify --json`，选择一个阶段 |
| 构建与阶段产物 | `zil build path.zil --output out/program --emit-dir out/stages` |
| 运行普通入口 | `zil run path.zil`；需要 JSON 包装时加 `--json` |
| 本轮限定回归 | `cargo test --offline --locked -p zil-driver --test impl103_generic_aliases --test impl105_ct_enum` |
| CLI pipe 回归 | `cargo test --offline --locked -p zil-cli --test stream_io` |
| 冷启动定位主树 | `node tools/handoff.mjs --check` |
| 集成者统一验收并发布摘要 | `python3 tools/validate-draft.py --publish-summary` |
| 包检查 / 交付 | `python3 tools/package_check.py` / `python3 tools/package.py /absolute/path/zil-source.zip` |

CLI 区分 Success、SafeReject、Unsupported、ResourceLimit、InfraError、Cancelled。空测试、缺 fixture、预算耗尽、超时和工具失败不能计为 Pass。作者跑限定测试，最终全门与精确收据由集成者执行。

`zil qc` 使用 Rust Roam 的 session、scalar-native、resource-closure-native 和 byte-buffer-native 四个 profile，共用反例、冻结重放、缩减与独立修复队列。拥有字节/2合同加入四种载体及真实分配/释放观察；本轮自然48个唯一程序没有发现生产缺陷。完整状态图、逃逸及并发不在该有限模型内，实际覆盖与发现归属见 [健壮性说明](docs/ROBUSTNESS.md)和[本轮审计](docs/DRAFT22-ROAM-AUDIT.md)。

主源码 ZIP 可直接构建，Git 历史与选定 raw 作为旁置附件。接续原审核链时按 `PACKAGE.json` 的历史附件恢复 checkout；不要重新初始化 Git 冒充原历史。见 [交付说明](docs/DELIVERY.md) 与 [Codex 接续入口](docs/CODEX-START.md)。

每轮按 [Rust 四目标](docs/RUST-GOALS.md) 写明证据与缺口。真实动态输入、多模块组合及有限配对成本已经测量；完整产品安全、运行与内存同级、维护优势、整体编译优势和至少一项产品级突破仍未共同满足。

## 工作区

| crate | 职责 |
|---|---|
| zil-base / zil-ir | 身份、诊断、目标、限额；未核 Core/Typed/候选/CT/LIR 与有界 wire |
| zil-syntax / zil-expand | 语法、源映射、模块名称归一、记录/接收者/管道/别名/有限实例化 |
| zil-types / zil-analyze | 类型、CFG、闭包形状；初始化、来源、窗口和转交候选 |
| zil-verify | 独立核验、私有已核包装、CT 和 foreign 边界 |
| zil-ct | 已核纯值 VM 与源码 CT 展开 |
| zil-lower / zil-backend-llvm | 已核 LIR、机器布局、LLVM 与链接 |
| zil-db / zil-driver | 查询、缓存与取消；项目和会话调度 |
| zil-cli / zil-ide | check/build/run/qc；复用同一 driver 的诊断 |

`_zil_design.md` 及具名附件定义语言；`_zil_goal.md` 和主树工单安排施工。历史文档、changes、预期样例和生成视图不能覆盖规范或充当实际成绩。
