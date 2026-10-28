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

真实源程序同时借 State 的 callback 字段与 Copy sibling，直接把后者传给189回调；对比共享/独占、相同/不同字段、最后使用、顺序重借、分支和循环。四模块消费者在 O0/O2 检查真实输出、两个拥有 Buffer 和 Ticket 的清理及每个分配失败点，使用直接字段地址，不继续保留局部回写替代实现。

最近反例覆盖同字段直接读/写/第二次借入、调用的当前实参冲突、共享升级、parent 整体移动/替换/清理、未初始化访问，以及仍关闭的非 Copy 字段形状。原始 TypedIR 和 Wire 独立伪造字段类型/索引/结果权限、Hold、初始化和父对象生命周期；不把手填IR等同于源面实现。实测预算下界与默认额度控制、限定回归、fmt 和 Clippy 分别记真实结果，首次失败不覆盖。

## Rust 四目标与终态

安全以字段与父位置的实际冲突和独立核验为证；运行成本以真实内联指针和分配/释放事实为证；表达能力以移除读出再写回的样板代码为证；编译工作以现有有界预算为证。未做同语义 Rust 运行/编译成本对比，不以测试数量或代码变短宣称全面优势。此片完成也不解除完整1.4、一般高阶借用或产品终态的剩余条件。
