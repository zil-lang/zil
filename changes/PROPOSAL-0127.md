# 127 泛型 helper 直接调用局部自包含工厂结果

## 当前缺口与接受范围

125 已允许普通 helper 调用已解析工厂并持有、清理返回环境，但 `let local=make(...); local(...)` 在 Expand 被 `called generic local is not the callable parameter` 拒绝。ZUI 的状态转换与事件适配需要把局部独立处理器和调用方回调组合。当前规范 9.3/9.4 已规定各 callable 的调用能力、拥有责任和 opaque 返回合同；本单组合这些能力，不增加语言权限。

本单仅开放一个有限非逃逸 helper 内，对已解析、自包含工厂返回局部环境的直接调用。支持 Fn、FnMut、FnOnce，各自依据自己的可见签名选择一次调用模式；FnMut 必须有可写位置，FnOnce 消费一次，失败不回退模式。未实例化泛型正文也检查参数、返回值、可变性与消费，不把 opaque impl 当作 Copy。

公共 helper 合同继续只描述它唯一的外部 callable 参数。其 trace 必须是实际参数来源调用的投影，局部环境调用不计入参数次数。来自同一工厂的参数和新局部实例具有相同 ClosureId；类型身份不能替代实际值/位置来源。Types、Analyze、Verify 分别重建参数与工厂局部来源，不共享安全判定函数。未知或混合来源保守拒绝，不能靠换名、Load/Store/移动/借用或相同隐藏类型漏掉参数调用。

## 既有独立合同和实现边界

局部环境仅能来自普通 Call 的已核 self-contained callable_return_contract，必须保持真实身份、空外部 Hold/Call/writes 与完整拥有 Cleanup。Analyze/Verify 各自既有 retained 检查仍先核验这些事实；局部每次实际调用及移动、借用、错误退出和清理由原资源与借用检查完整处理。不能仅信任生产者标志，也不能把任意未知闭包调用忽略。

不开放 helper 局部任意 MakeClosure、外部借入捕获、多个 callable 参数、helper 转发/递归或循环；不开放 `&local` 后间接调用。现有 borrowed helper lowering 专属于 F 的接口，本片不将其套用到局部工厂接口。工厂 prepass 的未解析结果快照和直接/间接工厂依赖拒绝保留。

每次新增来源扫描、表分配、传播和比较按已有工作额度先收费，不预建 CFG×来源积。公共完整 trace/exit 表示、schema/wire 17、ABI、默认额度与产品门不变。126 在途的节点额度诊断更正在最终验证前同步；额度不足应为 ResourceLimit，不能变成语义不安全或部分成功。

本单新增的未知/混合来源与显式借入局部工厂结果属于当前实现范围，Types/Analyze/Verify 均报告 Unsupported；不能把流不敏感来源精度不足叫作已证不安全。已经得到明确证据的 trace 错计、伪造返回合同与权限不匹配仍按原语义拒绝。

## 验收与最近反例

正例包括同一 factory 的参数/局部两个实例且参数 trace 恰一次；F:Fn 与局部 FnMut；两个独立 FnOnce 各一次；有限分支/错误返回与未调用环境。至少三个真实模块消费者在 O0/O2 检查数值、状态及清理顺序。

最近反例包括局部 FnMut 不可写位置、FnOnce 两次/移动后使用（含未实例化 helper）、参数别名及来源混合不能少计调用、factory 外部 Address/Desc 捕获/伪造返回合同/漏清理、局部与参数不同 kind 的合同篡改，以及局部 MakeClosure、显式 &local、循环和递归的既有边界。raw/wire 独立证据保留；新增工作做各阶段 N/N−1 与失败后恢复，不把缺证据或提前 Unsupported 当作通过。

生产写集为 Expand generic_check/generic_callable 与 Types/Analyze/Verify 各自 helpers；测试与普通多模块消费者另列。作者只做限定验证，独立审核绑定精确交付 SHA，root 负责集成。
