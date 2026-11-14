# Carrier / mixed ordinary assembly review

结论：本次只读源码复核未发现 mixed ordinary CFI / globals / calls 在 carrier→ASM 边界上的具体新增阻断。建议 root 正在补的一个混合 native 夹具作为该边界的实际验收；此前 rem251 全对象相等不能单独证明这个组合。strict 的 collapsed-if 与三个 FFI 返回类型不一致已由 root 接管，不在本报告重复派修。

## 身份和证据

- 用户指定候选：`d71ce38e556b2a8e5cfb7b235ad54f8ccd787ef3`。
- 已读 wave20 `out/runs/critical-path-wave20-d71ce38e556b/binding.json`：source 同上，tree `beb8b21e54e4fe08e2d4c7b7860d3a19f18606be`，SDK DSO SHA256 `785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2`。
- 已读同窗 `emission.json` 和测试源码：exit 0，14 passed / 0 failed / 0 ignored；当前 native 测试断言完整 ordinary object bytes 相等、9 reused / 3 distinct、无 fallback、current preflight receipt 与 cancel→retry。此报告未重新运行它。root 最新回报为 runtime gates 全过、strict 停止，不据此声称整窗全通过。
- 本次读到的文件 SHA256：`emission.rs` = `a066706c6d4de7f3e28ebfa87a5e6812409f789df50550f4a828f571bef06d34`；`emission_assembly.rs` = `580c846cdbe143e92eeab2f80054dc40dc03d07257e9df243b20e36e86b4914e`；`emission_assembly_tests.rs` = `c6e3568d8ba8cf7cced0cd005259c3b4150342f9341d1df52152fb53870f674e`。未执行 Git 或从 Git blob 独立验证工作树字节；候选绑定来自上述冻结收据，并以这些读取哈希限定源码范围。
- 本复核者未编写 root 的 carrier 实现；两份 ASM 文件此前由本复核者实现，故本报告不充作该部分的非作者独立签署。

## 具体边界核对

1. `emission.rs:399–527` 的 carrier 使用独立 context/module、同一 target machine，复制 source filename、target triple、data layout。private naked + nounwind 避免 carrier 自身通常函数序言及 unwind 区域包围 payload；这正是允许 payload 普通函数自行开启/关闭 CFI 的必要边界。rem251 实测支持该配置下没有额外可见 carrier 布局，但混合 CFI 仍需实际对象验收。
2. 完整 payload 每个 `$` 都加倍后交给 LLVM function inline-asm 模板，覆盖立即数及字符串中的美元；普通 `%` 寄存器不作替换。末尾显式 `.text` 将 carrier 的结束标签放回代码段，避免原 payload 以 `.data` 或 `.note.GNU-stack` 收尾时污染结束位置。reserved carrier 名碰撞直接拒绝。
3. `emission_assembly.rs:parse` 按完整普通函数范围复制，跟踪 `.cfi_startproc/.cfi_endproc`；即使 endproc 在 `.size` 后也纳入原段，克隆只能插在闭合之后。普通函数中的 call、CFI 和引用不经过克隆体的 leaf whitelist，也不被符号替换。global/directive 间隙按原字节、原相对顺序复制；克隆在后继 global 切换 section 前插入。
4. 克隆体本身仍拒绝 CFI/EH、section 切换、外部调用/符号或全局重定位；仅局部标签及函数 own symbol 精确 token 改名。全模块只去掉原未编号 module `.file`；编号 debug `.file` 保留。原函数、global、调用目标不会因 clone-name 前缀被字符串替换。
5. 普通函数/全局出现不支持的整体形状（例如生成 `.set` 或额外 function 定义造成定义序列不匹配）会整次回退。assembler 返回错误或 error diagnostic 也拒绝 speculative 输出；`native.rs:797–844` 从未被 linkage 改写的原模块走普通 object emission。此为覆盖率边界，不应记为 reuse 成功。

## 单一有意义的混合回归夹具

接受 root 当前方案：直接扩展已有 structured rem251 full-object 测试，不增独立框架或矩阵。保留 12 个 `zil.fn.*` / 三类封闭体、noinline、预期 reused=9；在第一组三个 canonical 与后续重复定义之间插入一个普通外部可见 wrapper，使后续克隆紧跟普通函数 CFI 结束区域。wrapper 调一个未定义的 external callee，并且继续计算后返回，阻止尾调用；读取一个外部可见、可变 live global，令读出的值跨调用存活并参与返回，防止 O2 消去 global 使用及 frame 需要。

该单例的非空前提必须从实际 optimized ASM/object 确认：wrapper 确实带 CFI/FDE，external call 仍在且有相应符号/重定位，global 定义及实际访问仍在；不要只根据输入函数意图推断 CFI 已发生。若既有模型方便表达，可在同一夹具加一个外部可见 pointer global 指向被恢复的 clone，从而同时覆盖数据段→clone 的地址重定位；这是同一夹具的加强项，不要求为此扩模型。

继续以完整 `ordinary.bytes == reused.bytes` 为主门，同时要求 reuse receipt 非空、`fallback=false`、`reused_functions=9`；对象全等包括 symbols / code / data / relocations / unwind，不能改为只比 `.text` 或行为输出。保持已有 current proof 与 cancellation 检查即可，不需复制新的取消测试。

注意现有 `native.rs:519` 只对 `Input::StructuredEmission` 请求此优化；仅增加普通 `Input::Text` 编译再比较对象不会覆盖 carrier。root 已选择扩 structured 测试，可沿现有真实入口完成，无需生产 hook。

## 范围

仅源码与已有小收据阅读；未运行 compiler、build、benchmark、Git，未修改源码。唯一写入为本报告。混合 native 夹具及其执行由 root 所有，本报告不预先声明其通过。
