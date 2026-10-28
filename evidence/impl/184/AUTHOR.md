# 184 作者交付

源码冻结为 `b1461dc39bd920770885b27796068a560e1e6a2a`。本目录只含文本输入、阶段表示、接口、LLVM、原始命令和结果，不含执行文件；24个排除文件的摘要另列。详细统计和范围以 `SUMMARY.json` 为准，所有最终命令的 source_commit 和干净状态均绑定该冻结提交。

## 已有实证

新片13项源码组、6项raw/Wire/预算组全部通过；限定相邻回归合计119项通过。新片20次O0/O2源码原生执行检验真实Copy环境槽、独立复制、地址写后读取、复合字段/数组切片、条件来源、参数转发、借入helper、State回调字段及拥有ByteBuffer。四模块consumer另有38次原生执行，含20次逐点失败；输出、退出码和实际分配/释放责任均与执行前的oracle一致。

11种独立raw突变各经直接Verify和Wire入口拒绝。它们覆盖伪造来源、激活栈地址、按值/消费调用、擦除Hold、共享写、将拥有写入口冒充Fn及在结果活跃时提前销毁环境。Types不作为这些入口的安全核。Analyze两个公开lending向量的长度在指纹遍历前受限并收费；opaque合同必须与实际定义一致，精确预算边界保存在cases中。

## 保留的失败及未完成项

开发中首次FnMut活跃结果再调用曾误获Success；原输入和完整阶段表示在 `preserved/initial-fnmut-loan-hole`。原因是旧贷款快速路径跳过纯Copy环境，已在冻结源码修复。Rust rlib缓存基础设施失败、源例中的裸block/缺manages/私有字段/重复FFI，以及精确诊断断言与Clippy修正都保留原命令日志。

旧099源码 `escaped()->impl Fn()->&[u8]` 从拥有ByteBuffer环境返回view，现在正确成功；该旧negative断言仍使legacy-boundary命令为6过1败。作者没有改旧测试，原输入和结果保留，交root做明确迁移。handoff检查依主树当时状态退出1，包含历史缺失和在途工单快照问题，不当作正式验收通过。

本片仍不接受一般life/HRTB、外借Address/Desc捕获冒充环境所有权、FnOnce/消费或临时环境借出、Package/Closure嵌套拥有字段的lending，以及一般非Copy调用参数。泛型State函数再调用泛型callable helper仍Unsupported，consumer01原尝试保留；最终consumer在State内直接借入字段调用，外部shared helper依然真实使用。

运行/内存与Rust同级、维护成本优势、编译速度优势和产品净突破都没有本轮配对测量。此交付是可执行语言增量，不能据此把完整语言或产品门标为完成。独立root审核与合流仍是后续步骤。
