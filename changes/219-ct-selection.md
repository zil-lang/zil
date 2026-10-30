# 219 草案：源级 `#if` 与已封闭纯标量条件

状态：2026-10-30 root 已冻结并派工；尚未实现或运行验收。调查基线 `63b5d064f443e9ed7989e0ec87d4e18b540a370a`；依据当前 `_zil_design.md` §16、§16.1、§16.2、§17、§19 的阶段与独立核约束，及 217 的合法 `ct-item-if` / `ct-statement-if` 原拒绝输入。旧 218 的聊天成绩与本片无关。

## 目的与范围

`#if` 已有 parser AST，但声明在 Expand 被标记为 compile-time item generation Unsupported，语句在 generic AST 预算遍历中也明确拒绝。声明式 ZUI 需要在选定构建配置下发射不同的声明、初始化与资源步骤；仅让 `#(expr)` 算出普通数值不能完成这个需求。

本片开放模块声明和普通函数/闭包/控制块内的 `#if`。条件支持当前已核纯标量计算，并允许调用**无条件、已封闭的普通纯标量函数及其有限依赖**；不只实现字面 true/false。布尔结果必须是实际 CT 执行所得，调用、循环、递归、短路和整数语义沿现有 CTIR → 独立 `verify_ct` / `verify_ct_program` → VM。函数自身是否无副作用仍由实际 CT 合同及 verifier 核验，源索引不颁发纯度证书。

条件函数首片限非泛型、无捕获的具名普通函数，参数/结果为现有 Unit/bool/u8/u16/i32/i64 的有限标量入口，运行期 managed/地址/闭包/FFI 不可进入 VM。可调用自身模块无条件 helper；跨模块只通过普通已解析 import 和实际 public 可见性。条件依赖 `#if` 发射的声明、含生成指令且尚未物化的函数体、布局查询、trait/impl 可变候选集，须明确报阶段未就绪；不能靠删掉相关正文、补常量 stub 或反复展开重试获得答案。普通递归函数的执行深度/步骤限制不冒充生成阶段环。

`#for`、`#emit`、Syntax VM、Type 生成、任意 CT 全局常量/反射、泛型编译期值绑定、条件 import、完整 cfg 属性执行不在本片接受范围；它们继续保留显式 Unsupported 及后续责任，不能据此宣布完整 §16、声明式 `ui!` 或全 ZUI 已完成。

## root 已裁定的生成列表语义

声明和语句 `#if` 的分支大括号仅作生成定界。选中声明或语句按原顺序物化到接收列表当前位置，不新增运行期词法作用域；名字、shadowing、defer、return、break/continue 和清理均等价于在该位置手写同一列表。声明重名及局部重复声明交由现有普通规则裁决。该选择来自 `#if` / `#for` / `#emit` 同为生成入口的一致性，须先由 root 写入 §16 的当前规范。

两支均完成词法/语法检查，只选中分支进入名字解析、类型与安全检查；未选中分支的合法语法可以含不可用名字或类型，但语法损坏仍 Parse 拒绝。未选中分支中的条件、foreign、资源操作和生成器不得执行。**禁止用运行期 `if (true)` 模拟选择**；它会改变作用域、控制流、返回完备性与清理位置。

## 最小实际公共源

`settings.zil`：

```zil
pub fn slots(rows:i32, columns:i32)->i32 { return rows * columns }
pub fn compact()->bool { return slots(2, 3) <= 8 }
```

`main.zil`：

```zil
import settings
#if (settings.compact()) {
    fn width()->i32 { return 6 }
} else {
    fn width()->i32 { return unavailable_branch() }
}
fn main()->i32 {
    #if (settings.slots(2, 3) == 6) {
        let value = width()
    } else {
        let value = unavailable_value
    }
    return value - 6
}
```

须同时覆盖参数改变使另一分支被选中的独立冻结源、多个层级 helper、实际 if/loop/递归的 CT 计算、短路不求值未执行除零、非 bool 条件拒绝。上述是拟定源码输入，不是已经通过的程序。

机器消费者为三个普通模块：配置纯函数、现有资源/ByteBuffer 操作、配置选择的初始化与事件处理。两种源配置都实际构建、独立 Verify 并运行 O0/O2；运行输入驱动正常/错误/提前退出，现有 linker wrapper 逐实际分配点注入失败，记录精确输出、退出、alloc/free/live 与接收块清理顺序。配置条件本身不接收运行期输入，也不在机器入口执行一次备用条件。

## 实现结构与阶段顺序

1. 保留原 ParseResult 和完整两支 AST/源位置。新的只读预检识别生成条件，按现有限额在扫描/索引/复制/队列增长前计费；没有选择指令的路径保持原行为与旧预算控制。
2. 从源中冻结 ConfigReady 查询快照。只选择无条件普通函数与普通 import，不混入条件声明；函数正文必须保留原字节语义，不构造替代正文。对已封闭函数建立实际声明身份/模块/可见性索引；生成中或尚未就绪的名称单列，给阶段诊断。条件仅允许读取这个不可变快照。
3. Expand 提供受限“表达式到未核 Core”桥接，driver 把条件放入普通 bool 查询入口；复用 `ct_program::Compiler` 和 `zil_ct::evaluate_source_constants_with_compiler` 的真实编译、独立核验与 VM。不能向 expand 添加 ct/types/verify 依赖，不能复制一套宿主 i128 布尔求值器或借用 cfg 的 Option<bool> 近似求值。
4. 取得实际 bool 后，仅递归处理选中列表。先预费再复制节点/名字/字符串；产生普通 AST，完整保留原 source spans 和选择轨迹。所有条件查询读取同一已封闭快照，选出的新声明不会反向改变本批条件的候选集合。
5. 完成选择后，运行现有单模块/项目名称正规化、泛型处理、Expand、Types、Analyze、独立 Verify、Lower、LLVM API。选中声明的 public/nominal 身份按普通模块规则决定，绝不按同布局或生成位置忽略原模块身份。
6. 记录 `selection` 产物：原条件 span、选中分支、条件依赖精确身份与实际 CT 使用量。只记录事实，不向 TypedIR 增加安全权限。Core/schema/Wire27 不因纯源转换而增加伪指令。
7. driver 的 Expand 查询现在可能执行目标相关 CT，键必须绑定实际 target；原始源码、规则版本、所有限额与依赖仍入键。命中选择缓存不跳过后续当前独立 Verify；`SourceMap` 与 IDE/build/check 的选择结果必须一致。

### 关键实现风险

- 当前 `modules::specialize` 的 generic budget 会在 MetaStatement 上报 Unsupported，所以选择必须位于完整泛型正规化之前。不能先正常展开完整模块，再删掉未选中代码。
- 当前 driver 先解析普通 import 图，故本片条件 import 明确关闭；不能在已缓存 import 图外偷读新模块。普通无条件 import 的源内容完整进入项目指纹。
- 当前 `ct_program::Compiler` 只收集实际依赖体，但它的输入声明快照不可混入尚未选择的结构。快照应以真实、无条件、闭合 scalar 函数为边界，不能先物化包含 #if 的完整运行函数来取得查询体。
- 若未选中合法语法包含特殊属性或未知类型，快照构建不能触发其语义校验。若已选中列表含尚未执行的属性，仍沿现有属性 Unsupported，不能吞属性。
- 现有简单 CT builder 对 `&&/||` 明确关闭，必须走 `ct_program::Compiler` 的 CFG/VM 分支语义，不能降成 eager 两边求值。
- 不把函数递归等同阶段依赖环；前者沿现有 VM 深度/步骤上界，后者由已封闭快照排除并给明确诊断。

## 配额合同

所有条件与原有 `#(...)` 共享一次请求/项目剩余 CT 步数及生成节点预算，不能每个 #if 重新获得默认限额。AST 预检、符号索引、依赖遍历、名字比较、复制、选择轨迹和输出列表在增长前预付 `max_analysis_steps` / `max_nodes` / 深度额度。选中输出和临时查询快照分别计量，不能仅计最后小结果而忽略被复制的 helper 体。预算不足为 ResourceLimit；不改到运行期、不交付半个选择结果、不提高默认限额。

验收新条件链与项目累计 N / N−1 / fresh N；无选择旧控制保持既有判定与预算。如果不可避免增加旧请求工作，必须在 root 的规范迁移中逐项记录并保留原输入/原结果，不得把新阈值写成旧执行。

## 建议精确写集

219 实施者可独占：

- 新 `crates/zil-driver/src/ct_selection.rs` 及必要的 `ct_selection/{context,materialize,budget}.rs`。
- 新 `crates/zil-expand/src/ct_selection.rs`，仅未核表达式桥接/源转换接口；不修改 receivers、generic_callable、generic_state 或 closure helper 语义。
- 新 `crates/zil-driver/tests/impl219_selection.rs`、`impl219_budget.rs`、`impl219_ct_boundary.rs`。
- 新 `tools/language/219/**`、`examples/mainline/ct_selection219/**`、`evidence/impl/219/**`、`docs/LANGUAGE-219.md`。

root 单写的共享注册/调用接点：

- `crates/zil-driver/src/lib.rs`（mod 注册），`session.rs`（单源码阶段/缓存/预算接线），`project.rs`（项目快照/选择/累计预算/源映射接线）。
- `crates/zil-expand/src/lib.rs`（mod 与公开无权限桥接）。
- `_zil_design.md` §16、`spec/interfaces.json` 对应源转换合同、当前 normative input registry、`.task/impl/219.md` 与 `_zil_goal.md`。

原 `crates/zil-driver/src/ct_program.rs`、`crates/zil-ct/src/source.rs`、CT verifier/VM 为消费依赖，首选不改；若调用当前接口不足，先向 root 报具体缺口后缩小变更，不和 218 同时改公共 callable 元数据。

## 验收族（需冻结输入，以下尚未运行）

1. 原217两个 `#if` 拒绝源原字节保存，分别晋升；正例包含纯函数/依赖链、比较、checked数值、短路、有限loop、两种配置、多模块 public helper。
2. 列表语义：分支声明局部在后续可用；相邻重复声明按普通规则拒绝；选中资源在接收块退出才清理；return、break/continue、defer 如当前语言已支持则与手写控制等价。
3. 两支语法都检查；未选中非法名字/类型、未执行 foreign/资源/生成条件不进入语义或执行；选中对应输入正确拒绝。
4. 非bool、运行局部、foreign、Managed、布局/trait查询、条件生成helper、函数体含未物化#if、条件import；按实际阶段和原因拒绝。保留 #for/#emit/Syntax 边界。
5. 条件整数溢出/除零、递归或循环配额耗尽、重复条件累计耗尽，以及 fresh 请求恢复；不把失败当false。
6. SourceMap 原条件/选中节点位置与源一致；普通单模块/项目API、check/build、缓存冷/命中/源改动/target改动/限额改动一致。
7. 真实源到独立 Verify 与 LLVM API O0/O2，配置两端和运行失败清理分别留证；生成的资源程序仍通过普通最近反例拒绝双消费/悬空借入，不新增 raw 安全权限。
8. root 完成独立审核后再做组合全量，218/219各自精确提交保留；作者不跑正式全量，不自行合main，不将本片通过升级为全部ZUI/1.4或Rust四目标完成。

## 其它候选未选的原因

只读 Product 借入不只移除 `closures.rs` gate：flat Package 默认借入也关闭，Verify 对 package address capture 明确拒绝，需要独立父根、视图保活与别名窗口合同，与218闭包写集重叠大。

Product 作为 Store 元素又牵动 push/pop 的 Product ErrorUnion、独立 union 责任/来源与递归 prefix drop。当前 LLVM Store drop 只扫直接 Managed/Closure，直接打开准入会漏 child 清理。应作为后继真实纵切，不在本轮以单个资格谓词修改冒充完成。

## 正式派工补充

218由language218实施前端，220由root实施Analyze与集成，221由language_review_plan实施Verify，219由language_parallel实施源选择。精确独占写集以主树工单为准；219的session/project/ct_program接点已纳入其独占写集，私有模块注册随所属单一文件施工并由root统一发布。交叉独审与组合回归保持。
