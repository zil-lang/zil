# 239–244 普通完整拥有Sum

Q实际探针已证明Copy enum、无Store嵌套Product错误载荷及两侧平坦拥有错误联合可用。缺口是含Store的完整Session进入错误载荷，以及普通enum承载完整拥有状态。原输入、预期和失败位于evidence/planning/language-next-Q，不改源规避。

接受性、指令、清理与预算合同见_zil_design.md §8.1.5。采用有界本地typed path观察、原子多local拆包和真实临时值发布；无guard，不定义拥有view。递归右payload清理通过整个pattern最终绑定逆声明序发布保持，不新增父年龄继承。不同同型目的的合法置换按实际字段映射保存来源，不能把合法置换本身当反例。原Copy泛型enum继续可用；拥有泛型enum和其他profile授权不由此自动开放。

公共schema/Wire30由239单写，op90–93；原v27/v29字节保留，未知opcode测试迁移至94+。影响Syntax/Expand/Types、Analyze、Verify、Lower/LLVM、CT明确禁止与全部公共指令消费者；根schema/interfaces/卡/规范注册表由root管理。Enum/ErrorUnion原ABI保持，无隐式分配。中间编译失败不算候选通过，五实现区域及独立消费者必须共同固定提交后验收。

分工：239公共IR/CT机械、240前端、241 Analyze、242独立Verify、243机器层、244真实消费者与独审。原Q3446/3347正式回归、236原样消费者、旧预算和声明权限都是回归基线；正式全量仅root执行。实现方sol/high，复杂Verify与独审sol/xhigh，独审者不写生产源码。

原设计讨论仅作历史，当前正文和接口优先。特别是旧草案的guard、新父年龄、裸SSA Unpack及只两条opcode均未采纳。最近反例包括旧tag在重填后比较、绕过成功边、多次Unpack、活目标覆盖、owning Read、来源洗白、活Store窗口跨切换、构造后项失败提前清旧值及递归右payload误序。
