# 072 package 字段的源语言操作与独立生产侧分析

2026-10-24（Asia/Shanghai）。源基线 `0c093f5e9fad117b8913d912020e48e9040d581a`，派工基线 `c39fd1037964b86e05d1bf706da138b69389797d`。规范摘要 `e79373ba49702c4fb40c38ceec7e6c3c1fb29ea32912c6452c83b7ca763c72e6`。依照主树 `.task/impl/072.md` 与已冻结 `changes/IMPLEMENTATION-0074.md`，在生产源码修改前记录本片。

## 范围与语义依据

依 `_zil_design.md` §§7、8、9、15.2：责任只能有一个实际载体；RHS 准备成功后才能清理旧字段并发布新责任；完整聚合按声明序清理；含用户整体 teardown 的部分移动须走已核 decompose。本片只处理现有有限 Package：直接 Managed 资源字段与有限 Copy 字段，不开放整体用户 Drop、嵌套拥有包或拥有闭包字段。

源语法复用 `give p.field` 与 `p.field = rhs`。拥有字段移出允许 `let`，赋值要求 `var`。目标限直接命名局部 Package 字段。Managed 赋值先计算 RHS 并建立真实临时责任，再依次发出 `DropPackageField`、消费准备临时和 `StorePackageField`；旧字段条件清理，live/moved 汇合可恢复。Copy 字段直接写入。若 RHS 转走整个外壳则提交失败；若仅移出同字段再补回则合法。可恢复 RHS 失败不提前清理目标旧实例。现阶段 Drop panic 仍走既有 abort 语义。

Analyze 独立维护外壳与直接 Managed 字段状态。部分外壳可读 Copy 及仍活字段；整个值移交、参数、返回要求每个 Managed 字段在全部路径 LIVE。字段旧 Desc 的未来真实使用阻止该字段失效，兄弟字段 Desc 不误伤；whole 移动/清理覆盖全部字段根。字段不进入局部初始化顺序矩阵，真实槽/集合/遍历收费；无 Package 函数不增加全模块扫描。

普通函数及闭包体内自建 Package 局部可使用这些操作。外层 Package 的拥有字段直接捕获仍不开放；不能把应该发生在闭包创建点的所有权移出推迟到调用点。先绑定字段 Desc 再由闭包捕获保留既有能力及实际 liveness 检查。一般嵌套拥有环境与已核 decompose 不在本片完成声明内。

## 实施与验收

仅修改派定 Types/Analyze 与专属源码测试，语法解析不加关键字。消费 074 发布 schema12 的三个公共原语；不复制公共定义，不调用 Verify 的求证实现。独立 Verify 由 073 实施，后端与 native 由 074 完成。

限定作者测试覆盖：直接/参数字段移出，部分包退出、Copy 读取/直接赋值，填满后 whole 转交，活字段覆盖与空字段补入，RHS 自取回填，分支条件 take 后替换，循环 take/refill，兄弟 Desc 跨替换，旧 Desc 最后使用后替换，晚实参/RHS 失败下的准备责任。最近反例：重复移出、部分整体移交/返回、不可写补入、旧 Desc/捕获未来使用、分支/循环未恢复、RHS 移走外壳、Copy give、Desc 冒充 Managed，以及外层 Package 字段拥有捕获。

所有限定运行保留原始 stdout/stderr 与精确命令；失败不覆盖。作者不合入、不改正式工单/产品门、不跑正式全量。独立审核绑定提交；真实 native 清理顺序与完整流水线由集成者验证。

## 独立审核修正

073 的非作者最近反例发现，闭包体内自建 `let p` 后写 `p.a.n`，源码约束遍历把 payload 写权限传播为包壳写权限，而同一普通函数可正常操作。修复在直接 Managed 字段的描述投影处停止包壳 mutability 传播；`p.a = ...` 和 `p.n = ...` 仍须 `var`。这恢复 §15.2 的既有权限区分，并加入闭包/非闭包一致性、部分包兄弟 payload 写及直接字段仍拒绝的六个源码近邻。不扩展嵌套 Copy place、包拥有捕获或一般 decompose。

第二项独立反例把同一个宽 Tuple SSA 重复存入 Copy 字段，发现 Analyze 只在输入预检付过一次类型成本，资源数据流每次执行 `StorePackageField` 的精确类型比较和 Copy 分类没有按宽度收费。修复在每次 Package 字段的递归类型操作之前，按当次声明类型的结构、名义/字段名字节以及检查次数预留工作；计费遍历自身也收费。Read/Describe/Take/Store 与 MakePackage 字段组装的类型比较覆盖于此；没有递归类型比较的 Drop 不引入 payload 扫描。预检已有最大深度约束仍先执行，常量/集合收费与该类型工作相加，不靠 Verify 的预算限制替代 Analyze 的独立边界。没有 Package 指令的路径不调用新增计费函数，不增加全函数或全类型扫描。

预算复验使用公开 Analyze raw 入口，固定相同 SSA 并交叉改变类型宽度与字段操作次数，覆盖 Copy 写、Copy 读、Managed 移出/补入及字段 Desc 读；每个组合测最小可接受 N 和 N−1 的 E-RESOURCE-LIMIT，要求新增操作的预算增量随类型宽度增加。保留非作者原始失败探针与数值。已有无 Package 精确预算控制必须保持通过。
