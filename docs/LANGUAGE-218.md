# 218：单一自含 F 的普通拥有工厂前端

本片实现一个按值接收自含 `F`、返回新 opaque callable 的普通公共工厂。当前交付限于 Expand／Types；220 的 Analyze、221 的独立 Verify、真实机器清理和非作者组合审核由集成方分别留证。本文不把前端接受计作完整语言、ZUI 或产品验收。

规范身份以主树 `_zil_design.md` §9.4.1 与主树工单为准。2026-10-30 消歧后的 `spec_hash` 为 `de431dc8632119e6bca306bd8c48b5497024f2c1330d37a600af9d52c6b16702`，`normative_hash` 为 `e4bd2d0f939a205b01618c0d5ce53a8725549be093db2658272633021131e44d`。不继承不可恢复旧 218 的源码或测试成绩。

## 实际 API 与边界

```zil
pub fn wrap<F:Fn(i32)->i32>(f:F,bias:i32)->impl Fn(i32)->i32 {
    return move (x:i32)=>f(x)+bias
}
pub fn once<F:FnOnce(i32)->i32>(f:F)->impl FnOnce(i32)->i32 {
    return (x:i32)=>f(x)
}
```

输入只有一个直接 `F` 值，声明可以是 Fn、FnMut 或 FnOnce。伴随参数沿用有限 Copy、直接 Managed 和完整可捕获 Package；叶 Store 的原资格保持。输入 F 必须自含，返回值有新的具体环境身份。工厂可以先调用或销毁 F，再返回不保存 F 的新环境；它不承诺保留输入参数。直接 `return f` 仍拒绝。

同一 F 实例化共享代码身份；同型的两个运行时值仍有各自的位置和清理责任。不同 F 的完整环境身份进入实例键。工厂体可以转调另一个已 grounded 工厂并返回该新结果；公共工厂的三模块转调已经由真实源测试覆盖。

声明模式属于输入值、局部别名和捕获位置。实际 Fn 经 `F:FnOnce` 输入后，在嵌套捕获中仍按一次消费处理；同一 ClosureId 的另一个普通局部不因此失去自身 Fn 能力。新私有模式表以 LocalId、ValueId、capture index 为键，不以 ClosureId 作全局权限表。

默认捕获沿用实际使用推导：对 F 的消费或资源 `give` 可以产生拥有捕获；另一个只读 Copy 局部仍可能是借入捕获，不能因同一环境里存在拥有捕获而自动变成自含。`move` 明确要求拥有捕获。Package 的 manages 字段被取走且未回填时推导 FnOnce，回填后推导 FnMut。

有限 Copy 签名包含既有无环境 FunctionPointer／ForeignCallback；这没有新增 FFI 调用权限。具体 Closure／F 不能成为本片 callable 的输入或结果；Store leaf 对三类 callable 的既有限制保持。借入 F、多个 F、普通 T、State 输入、lending、dyn、receiver、foreign 工厂和隐藏额外环境仍在片外。

## 实现闭环

Expand 对未调用定义也检查声明能力、别名消费、捕获、循环回边及返回环境来源。Types 从完整实参身份实例化普通函数，读取已完成实例的实际返回类型，生成真实 `CallableReturnContract`；它不为拥有边界制造空的非逃逸 `CallableParameterContract`。

新工厂不会改变公共 IR schema 或 Wire 27。闭包仍使用已有嵌套环境表示；本前端没有引入隐式 Box、RC、锁或新分配协议。源派生测试检查拥有参数、不同返回身份、TakeCapture／CallClosureOnce，以及同型不同位置的模式差异。这些检查不能替代 Analyze、Verify 或 native 的责任与清理证明。

## 限定检查与原始失败

可复跑命令：

```sh
cargo test -p zil-expand --test impl218_factory --offline -- --nocapture
cargo test -p zil-driver --test impl218_factory_types --offline -- --nocapture
cargo test -p zil-driver --test impl218_factory_budget --offline -- --nocapture
```

每次实际命令、结果和源码身份分别记录在 `evidence/impl/218/CHECKPOINT-*.json` 与 `FRONTEND-BUDGETS.json`。A／C 的失败保留：普通类型键误拒 opaque 结果、旧 opaque 调度早于实例化、工厂体转调的旧类型参数统一均曾阻止真实正例。另一次测试链接了旧 rlib；仅清理本任务 zil-types 构建缓存后，同源码进入修复路径，工具链全局原因没有结论。

E 保留默认拥有捕获的初始误拒、Store 拥有捕获后复用的初始误接受，以及资源来源和前置计费修复。Product 的 `value.child.tag` 示例在固定旧编译器也得到相同 callable kind 拒绝，因此本片没有把它改成新的 Fn 接受。缺少 if 表达式花括号、对非泛型 store_new 写类型实参，以及语法不支持的嵌套 impl 反例属于夹具错误，不能记成生产缺陷。

## 实测预算

以下均验证默认输出等于 N 和 fresh N，N−1 只得到相应阶段 ResourceLimit。旧三源与 221 使用相同字节和逻辑文件名；旧代码固定为 `63b5d064f443e9ed7989e0ec87d4e18b540a370a`，独立重编译最小前端 harness。旧／新 Core 与 TypedIR 调试序列化逐字相同。

| 固定输入 | Expand 工作 N | Types 工作 N | Types locals N | Types functions N | 对照 |
| --- | ---: | ---: | ---: | ---: | --- |
| 既有 helper | 586 | 421 | 6 | 4 | 旧值保持 |
| 既有 State | 829 | 1441 | 8 | 4 | 旧值保持 |
| 既有普通 opaque | 3 | 462 | 7 | 3 | 旧值保持 |
| 新拥有工厂 | 724 | 770 | 12 | 5 | 新片边界 |
| 转调与嵌套捕获 | 1438 | 1556 | 18 | 7 | 新片边界 |

默认预算未提高；本有限对照不证明所有旧程序零成本，也不是编译速度比较。

## Rust 四目标与突破判断

| 目标 | 本片实证 | 尚未证明 |
| --- | --- | --- |
| Rust 同类安全 | 未调用定义、声明模式随真实位置传播、源派生自含性与最近反例 | 完整安全结论须结合独立 Analyze／Verify、机器错误路径与组合审核 |
| 运行与内存同级 | 沿用已有普通函数边界和内联环境，没有新增隐藏分配机制 | 无同语义 Rust 运行时间、RSS 或尾延迟配对 |
| 表达与维护更简单 | 普通公共单 F 工厂可创建新环境、转调其他工厂并跨模块复用 | 真实框架维护成本、一般多角色和完整拥有组合未完成 |
| 编译成本更低 | 新工作预费，有限旧输入精确边界和 IR 保持 | 无 Rust 编译性能优势证明，专项门保持 |
| 至少一项净突破 | 源级单 F 到新 opaque 环境的真实前端纵切已实现 | 产品四目标合取与可重复产品净突破仍未建立 |

完整语言 NotSatisfied、B0 Doing、产品门 NotRun 保持。当前工作没有修改 `validation/product-gates.json` 或把阶段通过代替父任务完成。
