# 331：普通 Copy 类型参数与显式 life 组合（实施中）

基线为 `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`。本提案引用
ZIL-ORIGIN、ZIL-MODES、ZIL-MODULE、ZIL-TRAIT、322/194 的既有责任；
须经 root 冻结规范后实施，不以本文件登记为已支持。

## 基线实测

原型正例为：

```zil
fn first<T:Copy,life a>(x:&'a T)->&'a T{return x}
fn main()->i32{let x=7;return *first(&x)-7}
```

在 target331/jobs1 构建当前 CLI 后，真实 `zil check --json` 得
Expand/Unsupported/E-UNSUPPORTED，首个拒绝为
`life is supported only by ordinary direct functions and concrete inherent methods`，
verification_runs=0。未用定义 `bad<T:Copy,life a>(x:&'a T)->T{return *x+*x}`
同样在该守门被拒；这**没有**证明数值能力已经符号核验，功能开放后必须再核。
两源位于临时 probe，完整原型及预期均在此记录，不归档大 raw evidence。

## 拟冻结的最小完整合同

第一片限普通自由 Direct 函数：一个既有普通 `T:Copy` 类型参数，加一个或多个
显式 life binder；声明顺序可交错，允许已有普通 Copy 伴参及一层共享地址参数。
首要路径为 `&'a T -> &'a T`，并覆盖已支持 Copy 类型 recipe、共享 slice 的真实
组合。life 不占普通类型实参数量：显式 `first::<i32>` 与从实参推导 T 等价，
无需也不接受伪造一个运行时 life 类型实参。实际 T 继续逐项满足原 Copy 资格。

实例化仅代入普通类型参数，并保留原 life binder、原标签、实际参数顺序和
结果关系；随后按 322 构造 CoreLifeContract 和实际参数上界，两核从真实 CFG
各自核验。life 每次调用的绑定不制造新的机器单态化；实例身份仍包含定义与
完整 T，名义/phantom/透明别名沿现有规则，不能按布局或 life 名字合并不同 T。

未使用的模板也必须在声明的符号 Copy 资格下检查，不能先换成 i32 再重检。
普通类型比较与 life 来源约束分开：结构一致的共享地址不靠标签拼写判断来源，
返回每条可能来源须属于当前函数返回 life 对应的真实输入集合；局部来源、错误
binder、重复/未声明 binder 与实际实例不符合 Copy 都拒绝。不同函数同拼写 life
不表示同一 binder，静态转借调用必须按实际参数位置代换来源。

首片参考来源流只开放当前可确定的参数、普通局部别名/重借/投影、分支合流及
具有已保留 life 关系的静态直接 helper。无法在定义期得到必要关系的转发/存储
入口须明确 Unsupported，不猜来源；不得以删除生命约束或借 i32 特化来接受。
不改变无 life 的旧正文推导，也不破坏 322 已支持的具体 inherent Self 方法。

混合 F/StoreElement/const、泛型方法与 trait lending、可变 life、HRTB/FP、
where life 及借入存储不随此片开放。首片 Copy 前提采用 inline 既有写法；
其它组合保留未来实现义务，不注册为永久安全拒绝。

## 必需消费者与证据

- Expand 区分普通类型与 life 名字空间、泛型计数、符号能力及来源检查；仅在
  许可头位置比较/实例化地址结构，原标签继续保留到 Core。全类型扫描不免费。
- Types、Analyze、Verify 复用 322 的真实来源上界核，不能共享求证函数；
  CT/模块/Self/生成重建、接口身份、source/live/restored cache 保留原边界。
- 沿既有 borrow target 验证多实际 T、推导/显式、binder 改名/参数换序、未用
  坏定义、错来源/局部逃逸、raw/CT/cache 身份与原反例；限定 O0/O2 和各新路径
  N/N−1/fresh，旧精确费用如新增实际检查而改变则保留原 Fail 后真实迁移。

预计无需新公共 schema 或运行时表示。共享 generics/generic_check 入口与 330
默认方法由 root 串行整合；额外必须调整 alias/trait-bound 中仅普通类型的词法
作用域时先向 root 报写集，不将 life 名字错误当成普通类型变量。

本片尚未实施，完整语言、ZUI、life/trait 终态及 Rust 产品四目标未完成；没有
运行/内存、维护成本或编译速度比较，也没有产品级突破结论。

## 第一阶段源码 checkpoint

规范已由root提交`e28e08d8825085e9a3c6bae57d2a85d3a2418481`冻结。当前接通
普通类型参数计数/代入与life保留，alias/Copy-bound名字空间排除life，并在
泛型定义核中加入实际参数来源、局部根、静态callee位置代换及分支合流；
循环暂要求外部引用来源不变，不能通过单次扫描猜固定点。实例仍沿322的
CoreLifeContract和两核独立CFG核。`cargo check -p zil-expand --offline`已过；
本checkpoint尚未完成原源/机器/缓存/预算限测，不记验收。
