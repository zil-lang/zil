# 205：一个 State 与独立拥有的同 F 替换值

状态：集成方以已冻结204交付 `4fa100e4b2f2cad5e480129263463a5914a36a5f` 为基线正式发布施工合同。以设计§8.2.3及interfaces.state_replacement205为准，注册不计实际通过。

## 新有限入口

在既有同 F State 的一个或两个直接 callback 字段之上，普通自由函数可接收恰好一个完整拥有的 State<T,F> 和恰好一个独立按值 F，返回相同完整名义实例类型的 State<T,F>。这里不要求原物理对象身份；合法 take/refill/重构仍按实际值和责任检查。可以有既有有限 Copy 参数及已经特化的普通 Copy T；两个拥有参数按声明位置求值和准备，顺序可逆、可以被 Copy 参数隔开。State 与 replacement 不得占同一实际参数位置。

新 profile 的 F 声明可以是 Fn、FnMut、FnOnce，完整 callable 参数保持已有有限规则，结果限有限 Copy/Unit。F 不因此成为 Copy。完整 State 参数与返回保留同一名义 family、模块、普通/phantom T、字段声明顺序及同一实际 ClosureId；不同工厂/表达式的具体 callback 类型不得通过反序实参覆盖约束而混合。旧单 State、旧双 F/G 和 lending 路径不反施新限制。

## Core 及源级责任

复用现有 Core 表：state_parameters 恰一 owner、一项 generic_index=0 的 F binding，selectors 精确完整覆盖该 State 的一或两个直接 F 字段；callable_parameters 恰一真实 replacement 参数，generic_index/name/kind/完整签名与 F binding 一致，传递模式为 owned。不增加公开字段、Typed opcode、ABI 或 schema/Wire 版本；新的有限接受性由本条与 interfaces 的具名 profile 登记。此为 203 State 表和 direct 表互斥的唯一新增例外。

同一个 F 类型槽最多对应三份不同运行时值：State 两个字段和 replacement。类型绑定可以共享，字段/局部/实参位置、初始化状态、别名代次、移动、借用及清理责任必须分开。不得以 generic_index 或 ClosureId 的单个位代替真实位置，也不得把 State 字段伪装成额外运行时参数。新 profile 中各 FnOnce 位置可独立消费一次；消费某一旧字段后可用 replacement 补回并完整返回。原字段二次消费、replacement 二次转交、旧别名跨 take/refill 复活、循环回边上重复消费必须拒绝，包括未调用定义。

## 求值、替换与安全核

原调用顺序和 callee-before-arguments 保持。完整准备 RHS 后才能作为替换提交销毁旧字段并发布新值；失败只清理已获得的责任。若参数准备临时已经取得完整 State，后续参数失败仍须正常清理该 State 的全部存活字段；这不是一次替换提交。成功替换后的字段保有独立责任，未使用的 replacement 仍按原函数退出规则清理。

借用窗口按实际字段/父 State 判冲突：同字段窗口横跨 replacement、whole State move、take/refill 后使用旧地址均拒；已结束窗口及不冲突 sibling 字段保留原精度。Analyze 与 Verify 从具体 IR、实际位置和生命周期独立重建，公共候选计划与源声明来源不授予安全权限。Typed owned_state 复用既有表示，不伪造同步 noescape helper 事件合同；raw TypedIR 内存安全检查不冒称恢复较弱源 bound。

## 边界、预算与验收

本片仍关闭两个 State 输入、借入 State 更新、多 replacement、F/G 混合更新、不同具体 callback 类型替换、嵌套 callback 字段、retained ordinary type_parameters、callable_return、foreign、receiver、opaque/dyn 擦除及增长递归。完整语言相应终态义务保留，有限片验收不删目标。

所有新元数据、名字、签名/类型比较复制、selectors、来源与位置状态及实例队列必须先计费；raw Core 两个公开 Types 入口核未使用头与定义。带普通 T:Copy 但未实例化的源定义也必须在 Expand 早期参数化检查核本片完整头与正文，不能等生成 Core selectors 才检查。CT plain/context 及 driver collector 核有界传输头和正文正规化，不承担源级 move/alias 证明；新双头完整签名比较必须在双方名字/签名预费后。CT 仅保留运行时模板，不执行，wrapper 也不能隐藏执行；原默认额度、旧无新 profile 的精确费用和旧 schema/Wire27 保持。

验收包含旧 impl200 state-and-replacement 的 138 字节原源逐字晋升、真实 Fn/FnMut/FnOnce 一/双字段 native、同类型三环境可区分 marker、反序和非相邻实参、不同 factory/phantom T、未调用 move/alias/loop-condition、raw Core 头伪造、source-derived Analyze/raw Verify/Wire 最近反例、CT 禁止、旧预算及新 N/N−1/fresh N、五模块 Linux/Wine O0/O2 与实际逐分配点失败；非法安全负例不 native。原 two-states/borrowed-inputs 不晋升。作者、非作者和 root 整仓分别保存精确身份。

## 正式派工

- root：主树合同/任务、CT、必要 driver/IDE metadata gate、Analyze/Verify 实际缺口、最终集成与全仓。
- roles196_frontend：Types 的 helpers/state_origins/inference/helper_plan/callable_fields；必要 lib/program/store_templates 在发布写集内；真实 source/预算与旧具名断言迁移。
- bundle_sync：Expand generic_callable/generic_check/receivers；必要 generics/generic_state 在发布写集内；真实多模块消费者及平台故障清理。
- impl197：非作者 source/raw/Wire/预算/消费者审核，不改生产。

204 已通过证据不重跑、不改身份；205 正式施工采用204最终交付作为基线。完整1.4 NotSatisfied、B0 Doing、产品/原生Windows NotRun，编译速度支线独立。
