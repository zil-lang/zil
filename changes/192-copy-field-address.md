# 192 — Package Copy 字段的实际借址

## 明确来源与范围

正式工单是主树 `.task/impl/192.md`，来源生产提交为 `bfe60582c46526a75349d4eefd3c60fde8f327e6`。独立实施树在 189 作者文本证据 `f577b71218370cdf706f7df1c414f8660f03ce0c` 后建立，仅额外吸收 root 的 192 派工文件；189 的独立复核仍由 root 完成。本片不把该候选提前视为主线验收。

189 原四模块消费者实际在 `&mut current.next` 被 Types 拒绝：原源码和首个诊断已保存在其证据。Copy 字段能读写却不能交给明确的地址参数，迫使普通代码先复制到局部、借给回调、再回写。规范第 2、3.3、3.4、4、7 章已分别要求源类别、物理字段来源、实际访问资格与初始化/清理；本片以现有表示补齐该能力，不引入新的生命周期语法或隐式运行时保活。

只允许一个已存在且已初始化的 named Package 局部中，已支持有限 Copy 字段的一层共享或可变地址。已有具体 Closure 字段借址保持。普通 Package 本身仍不可一般借址；Managed、嵌套 Package、Desc、Address、slice 等非 Copy 字段不因这项准入变成可借目标。var 与独占权限分开检查；共享窗口不能升级成独占窗口。

## 六处纵切

- Types `addresses.rs` 对真实 Package 字段生成已有 `BorrowPackageField`。Copy 字段没有 Closure 自包含检查；具体 Closure 仍按原规则核对。
- Analyze `resources.rs` 按实际字段类型与权限检查，Copy 依赖完整活动 shell，不生成或查询独立拥有债务；Closure 继续检查其真实活跃环境。
- Verify `packages.rs` 独立核验名义 Package、字段索引、类型、结果 Address 权限与局部可变性，不能信任 producer 的分类。
- Verify `resources.rs` 独立检查活动父 shell；只有真实拥有字段才查询独立债务。缺初始化、移动后或销毁后访问不能被 Copy 身份掩盖。
- Verify `borrows/loans.rs` 为直接 `ReadPackageField` 加实际读权限检查。既有物理 `Root::Field`、whole-parent 相交、写入/借入/失效、last use 与重借关系继续复用。同字段活跃独占借用期间直接读取也必须拒绝，不能只关闭写入。
- LLVM `emit/packages.rs` 对已核 Copy 字段发真实内联 GEP，保留原 Closure 路径；无新堆盒、拷贝回写、RC 或 runtime 借用检查。

沿用 schema23 和已有 Wire/fingerprint 编码。所有新增检查先按现有预算收费，不提升默认额度。内层 Copy record/array 可沿已有投影操作访问；不宣称对一个顶层字段内的多个子字段建立新的独立借用精度。

## 可证伪验收

实际邻接测试另外发现 `move` 拥有 Package 捕获中的 `&mut state.copy` 被旧 `mark_address_write` 一概当作共享捕获而拒绝。root 在 `bb2ca3b2` 明确追加 `closures.rs` 写集：仅将 Package 来源交给已有 `name(write=true)` 路径，仍由该路径要求 explicit move 并记录 FnMut；非 move、一般 Package 借址和其它来源权限保持。原失败、拥有环境原生对照与 raw 调用模式伪造另行留证。

真实源程序同时借 State 的 callback 字段与 Copy sibling，直接把后者传给189回调；对比共享/独占、相同/不同字段、最后使用、顺序重借、分支和循环。四模块消费者在 O0/O2 检查真实输出、两个拥有 Buffer 和 Ticket 的清理及每个分配失败点，使用直接字段地址，不继续保留局部回写替代实现。

最近反例覆盖同字段直接读/写/第二次借入、调用的当前实参冲突、共享升级、parent 整体移动/替换/清理、未初始化访问，以及仍关闭的非 Copy 字段形状。原始 TypedIR 和 Wire 独立伪造字段类型/索引/结果权限、Hold、初始化和父对象生命周期；不把手填IR等同于源面实现。实测预算下界与默认额度控制、限定回归、fmt 和 Clippy 分别记真实结果，首次失败不覆盖。

## Rust 四目标与终态

安全以字段与父位置的实际冲突和独立核验为证；运行成本以真实内联指针和分配/释放事实为证；表达能力以移除读出再写回的样板代码为证；编译工作以现有有界预算为证。未做同语义 Rust 运行/编译成本对比，不以测试数量或代码变短宣称全面优势。此片完成也不解除完整1.4、一般高阶借用或产品终态的剩余条件。

## 恢复后的中间发现（未终审）

恢复时已有7处生产修改未提交。development04原17组全部通过，保留原owned Package字段借址输入。development05补充检查后源17组仍全过；新raw组3过2失败：一条父清理输入更早被E-VERIFY-INITIALIZATION正确拒绝，需要精确校正测试诊断；另一条将实际有独占字段借址的拥有Package闭包从FnMut重标Fn，独立Verify错误接受。该事实说明既有Shared Package写集合漏列`BorrowPackageField { mutable:true }`，不能把此候选作为完成状态。接续在已授权的Analyze/Verify资源文件分别补实际绑定view的资格检查；无新增公共schema和默认额度变化。当前先提交可恢复WIP，后续独立冻验才出最终源码。

中间缺陷已在Analyze和独立Verify的既有Shared Package视图检查分别补齐：只对实际绑定捕获view的独占字段地址拒绝，普通函数或共享闭包内部新建的局部Package仍可取得独占字段地址。development06保持所有原输入，源18组及raw5组全过；raw误标Fn现在分别被Analyze和raw/Wire Verify拒绝。父对象已清理/未初始化保持原输入，断言改为实际更早的E-VERIFY-INITIALIZATION。该运行仍是未提交工作区的开发检查，最终事实以随后干净冻结执行为准。

## 独审实际用户资源兄弟字段反例

root 在生产等同 e362 的独立 cbeb1b 候选上保留了原输入：持有 `&mut state.count` 时，`give state.owner` 取走 R、补入新 R，再通过原 count 地址累加 old.n。该输入在 Verify 的 232..248 span 被 E-BORROW-CONFLICT 拒绝；TypedIR 没有 DescribePackageField，因此不是旧 Desc Hold。实际 count 地址是 Field(local4,1)，但 field_root 对 Managed(Record R) 仍按179旧保守门退成整个 Local4，令独立的 TakePackageField(local4,0) 被误判相交。

root 在89b1d7adea261d0596f988fa442d6c1cec38f14f明确追加 roots.rs 写集，仅允许已经独立核验名义定义的 Managed(有限Copy Record) 获得实际 Field(local,index) 根。保留 charge_payload 完整计费、全部失效检查、公共 HoldRoot 与真实资源实例规则，不把 Managed 当作 Copy，不开放资源字段借址。资源载荷不能持语言 Address/Desc，teardown 仍由独立资源验证要求匹配的单 Desc 参数和无环境普通函数。

修复原输入须 O0/O2 输出 AB、退出0；同资源旧 Desc 跨 take/refill、整体父对象移动清理、同 Copy 字段窗口冲突、未知名义或非Copy资源载荷仍须独立拒绝。旧 e362 收据不改写；新修复另冻验，仍按本作者树实际 schema23/规范快照记录，root 的 schema24 合流另验。

repair01原独审正例已输出AB/0；新增旧Desc源反例实际被更早的Analyze以“head transition invalidates a description required later”拒绝，首次19/1日志保留，原输入不改，仅校准预期阶段。repair02源20项与raw6项全过：同一资源Desc跨take/refill的raw/Wire另由Verify独立拒绝；未知名义定义与非Copy载荷也分别按独立资格拒绝。两次均为开发检查，随后以干净修复冻结的有界复验为正式结果。
