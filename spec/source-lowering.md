# Source 到 Body IR：zil-typeck 合同

本文规定 `zil-typeck` 的职责、内部层次与对外产物，作为 `spec/semantic-kernel.md` §2、§7 的下游细则。语言规则以 `_zil_design.md` 为准，本文不新增语言能力。

## 1. 职责与边界

- **输入。** 由 `zil-syntax` 解析得到的全部模块，包括项目模块与 `lib/std`。typeck 不读取 Expand 的产物，也不调用 Expand；旧 `zil-expand`、`zil-types` 只作为冻结参考。
- **输出。**
  - 一个单态化的 `zil_ir::body::Module`：所有函数的 `type_params = 0`；
  - 一份 `SourceMap`，作为候选证据，供 `zil-check::fidelity` 对照 AST 复核；
  - 完整诊断。
- **不授予安全准入。** typeck 只做名字、类型与结构判定。来源、初始化、借用与责任一律留给 Body IR 上的语义内核。为了让程序通过而插入的隐式转移、复制或延长生命周期，都属于错误。
- **CT。** `#(...)` 与 meta 项在已核验的 Body IR 上解释求值，与运行期共用同一份 IR 和内核验证。解释器是独立模块，typeck 只把 CT 片段降为 Body IR 函数并请求求值。

## 2. 层次

各层对上一层产物做穷尽匹配，新增的语法或类型构造必须在每层显式处理；未支持的构造给出 `Unsupported` 诊断，不允许静默跳过。

1. **resolve。** 建立模块图与项表（函数、聚合、枚举、trait、impl、别名、常量、extern），解析 import 与路径。每个项得到稳定的 `DefId`。
2. **collect。** 把声明降为语义类型：
   - 聚合与枚举成为泛型 `AdtDef` 模板，字段身份为“声明 + variant + 序号”；
   - 函数签名成为带类型参数和区域参数的模板；
   - trait 与 impl 成为约束和方法候选表。
3. **infer。** 逐函数做双向类型推导：基于等式约束合一，整数和浮点字面量带后备类型。方法与运算符按 trait/impl 解析，闭包推导捕获与种类（Fn/FnMut/FnOnce）。产物是 THIR：所有表达式带类型，所有名字解析完毕，place 表达式与 value 表达式明确区分。
4. **mono。** 从入口和 CT 请求出发，按需实例化泛型函数与 ADT。实例身份为“`DefId` + 完整类型实参”，实参必须封闭（不含推导变量）。实例化工作计入预算。
5. **build。** 把 THIR 降为 Body IR，结构参照 rustc 的 MIR builder：
   - 表达式分三种求值形态：place、operand、rvalue；
   - 临时量的生命周期按词法作用域收束，作用域出口显式生成 `StorageDead`；
   - 拥有值的清理显式生成 `Drop`，规则见 `_zil_design.md` §7；
   - 每条关键指令都记入 SourceMap 关系：绑定、字段选择、求值、资源操作。

## 3. 不变量

- **单一类型表。** typeck 只经由 `TyTable` 的生产接口 intern 类型；Body IR 中出现的每个 `TyId` 都必须在 OpSpec 约束下成立。
- **结构正确。** typeck 产出的每个函数都必须通过 `check_instruction`/`check_terminator`；不通过属于 typeck 缺陷，而不是安全拒绝。
- **语义交给内核。** 指令序列忠实表达源语义：求值顺序、移动与复制、借用、清理位置都按语言规则排列，再由内核判定是否安全。typeck 不预先过滤内核会拒绝的程序。
- **可复核。** SourceMap 中每条关系都必须能由 `fidelity` 依据 AST 独立复核；typeck 不得声明 AST 推不出的关系。

## 4. 推进顺序

先打通核心子集：标量、记录、枚举、元组、数组、控制流、match、引用、函数调用、`T: Copy` 泛型。随后依次是闭包、trait 与方法、Package/资源与清理、错误联合与 `?`、Store 等协议内建、dyn、CT。每一步都以真实 `.zil` 程序走完 Source → Body IR → solve → check 作为验收。
