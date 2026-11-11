# Bounded最终退役对象独立审核

**Pass：先发布A2 `5c97386cef5a337c432fc7f1351dfcc22a0c02e6`，再仅将bounded任务ref从1679推进到W `69a0aeb3cbd1f7cf7f992315f9bb1b33b9a73247`。** 本审核只读API对象及本地最终文件，没有触发workflow、修改ref、生产或原始benchmark，也未重验旧raw。

- W唯一parent精确`1679decf177c9a6eda5a64063c525f06e44e56a2`，tree `aa04999ebdb6dc0e80f681eb71ecf101e3a58457`；唯一diff为新增原退役workflow路径。
- 远端W、本地final-bounded.yml、A2保留recipe共享blob `fe4215efed9c2d52e0c5e83d281da14e075fc219`；6713字节，SHA256 `76bccdf5a692ee47db924115e9d3c642f888cecbcd47642d1f7a6a87c5fe272b`，无本地末尾LF差异。
- 名称、branch、OLD=1679、PRIOR=A1四字段反向替换后，与成功远端1dfd8b5模板逐字相等，次数[1,2,1,1]；Python AST解析成功，身份、归档直接父、manifest、活动run/PR、精确lease、404和receipt保护逻辑未改。
- A2 tree `b1f2eb0248af2ab2008580c95f9c945b56c2d2e1`，直接parents精确[A1 `a018e25431dc74a5d56e7e2df9ecf75b9f179717`,W]；相对A1只改manifest并新增final-bounded.yml和layout-retirement-receipt.json。
- manifest prior=A1、commit_parents=[A1,W]、commit_tips含W、bounded remote_refs明确为planned W；source_evidence_head仍1679、bounded retirement_completed=false，没有提前记完成。
- layout retirement_completed=true、completed_run=42988640504，收据实际入tree；其blob `d96c3cbed4494010ccf1db66ba35e4312adc4de0`与本地root成功收据精确相同。
- API快照中实际archive仍A1、bounded任务ref仍1679，符合先归档再推进的前提；旧完整证据经A1/E祖先保持，未覆盖raw。

允许顺序不涉及main、goal或已删除layout ref。推进后仍由原workflow重新核验活动工作、精确head和当前直接archive；本Pass不是预先声称删除已完成。

精确对象：[W](https://github.com/zil-lang/zil/commit/69a0aeb3cbd1f7cf7f992315f9bb1b33b9a73247)、[A2](https://github.com/zil-lang/zil/commit/5c97386cef5a337c432fc7f1351dfcc22a0c02e6)。完整核验身份见final-bounded-review.json。
