# 205 正式整仓首轮的实际返修

root 在 clean `0317a093ade0118741d334c45438ade07e9d7228` 执行完整 workspace，实际 2544/6/0，259组；原命令、所有stdout/stderr、完整输入与阶段文件保留。消费者Linux/Wine的有限通过不替代本次整仓结果，运行期没有改源码。

- 真正例回归：`impl177_state_composition::complete_local_assignment_restores_liveness_after_rhs_transfer` 的867字节原源 SHA `dc68299194fcc8dcea73fad37540a270bf16fe497235a8c6858d1bff750e199e` 在 `(next)=relay(next)` 出现 Types `E-STATE-ORIGIN`。不得将原Success断言改Reject；旧单State实际参数的完整Package不能被错误当成F字段的声明来源，必须沿真实字段统一。
- 原197未实例化借用反例：原267字节源 SHA `abf1ab9225dd61a989c14f56f119931d11cf8c6bc156f63fd202ce1be386dcf0` 过去省略正文后只核main；本片具名定义检查已正确在Expand拒绝旧alias。保持原输入，迁到精确SafeReject、诊断和Verify0，不再断言正文省略后Success。
- 原197实际held Copy窗口与原184借出payload后whole-parent move：均仍为同一非法源，现正确在Expand拒绝；合法last-use/sibling原生控制保持。只迁两处精确阶段/诊断，不放宽其它Verify检查。
- 原201当前调用窗口：同组前两个callee/parent负例继续原Verify断言；只有第三个same-element-two-exclusive-windows现由Types `E-STATE-ALIAS`更早拒绝。不能将整组允许阶段泛化。
- 原202固定201预算：首个原输入实际Types work6326、locals16，旧硬断言4241/16失败；后两输入尚待逐字实测，不能猜新值。此次旧State定义检查真实费用按当前9813具名修复条款记录，保原4241/11851/11792和全部原始失败，再逐输入N/N−1/fresh比较完整Typed。默认预算和产品条件不变。

此文件登记返修范围，不是返修通过或最终验收。作者、非作者及root分别保实际提交与完整输入；所有新阶段/预算断言需独立复核，整仓在共同冻结后重新执行。
