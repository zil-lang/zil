305 实际 layout key 精确冻结独审

结论：指定六文件完整增量未发现新增 blocker。实际 planner key、PartitionProduct/四槽恢复、当前 CodegenSummary、按 key 的物理 inventory、retained producer、ABI 与 aggregate 核账已贯通；不再以多个历史 layout 在 DCE 后物理相同来猜当前来源。

精确 commit `305175a125240f5dd7ae4d935352047d43992723`，tree `4adb25dc86d0f418f883f8ad72d46c303d1d4dc7`，基线 `ea5349ed069f91f8baaeeb811a421ca8c805bb6c`。已读六文件全部 diff（354+/40−），driver 测试实际路径为 `crates/zil-driver/tests/retained_native_publication.rs`。仅补读必要既有消费者；未读 1.204GB 归档或 implementation.md，未构建、测试、重放、计时或修改源码。

**Backend 身份链。** Pending::key 直接返回已有 key，不重算或创建权限。新 partition 从实际 pending_layout 借用捕获 key，再保存到同一 PartitionProduct 及返回元组；不存在 pending 时为 None。class 拒绝后选择普通成功路径的 Pending，不携带被拒绝 class 的 key。cached_partition 从实际被选中的产品复制 layout_key；当前槽与 completed 四槽使用相同分支及原完整匹配门。CodegenSummary 在 partition 选择后只初始化一次，JSON 输出该 key；before_codegen 与正常/late final/link 恢复成功出口均返回这份当前 summary，没有从旧 executable producer 覆盖 key。原四槽产品所有权和成功记忆路径不变。

key 捕获先于 pin/可选 commit，因此字段表示所选 producer layout 身份，不保证文件已发布。原取消、路径保护、成功后 commit 事务未改；缺失文件或未完成可选写入不能由这个字段授权。内存 partition 命中仍不补写丢失的 layout 文件。

**工具实际来源链。** report_layout_key 限制 null/hex64，实际 IR 有已验证 class 时强制非空。current inventory 只读取该 key 的 `.layout`，继续核目录/文件路径、header key、payload hash、实际 units、完整当前 items/closure、对象 ABI 与 protected exports；没有按 key 绕过旧验证。current_inventory_layout 再绑定 report、inventory summary、唯一选中归档、归档实际 bytes/units/payload，以及 current_layout_matches。旧无 class/no-key 主线兼容路径仍单独保留。

retained 历史身份增加 `(cache, normalized LLVM, full ELF, layout key)`，保存实际完整 producer inventory；恢复用该精确条目构造冻结 helper 的局部输入，helper 仍重核对象/container/符号/归档 layout。即使同 LLVM/ELF 的未带 key 历史已指向别的 layout，也不能串用。新的 key 或 diagnostic 尚无真实 producer 时明确拒绝，不能仅凭当前 layout 文件与同 ELF 伪造 producer；这可能保守拒绝合法终端复用，但不是质量通过。本次仍保留真实零 object lookup/native work 的统计范围。

**ABI 与核账。** executable_abi 重新绑定当前 report/归档布局，要求 16 个有序 unit_index，逐单元重新执行 object_abi，并从实际物理定义/undefined 重算 canonical removal、private imports 与 cross exports，对比 inventory 元数据。quality 在 aggregate 前再次绑定当前模型、layout 和 executable_abi 输出的 class_sources/key；原 extra/missing/dead-class 三条件仍在，未以元数据 hint 替代物理证明。原 Verify、full-O2、foreign/entry ABI、relocation 与完整 runtime 门未删除。

**源码回归覆盖与限制。** Python 新增两个 DCE 后物理兼容 layout 的按 key 选择、未带 key 历史被覆盖后仍恢复正确 producer、缺失/非法/伪造 key、归档 key 冒名、corrupt layout、错误 closure，以及新 key 无 producer 的保守拒绝。driver 新增 ignored/native-profile 回归含十次编译、七次私改转换、前两次重复输入的 exact partition/before_codegen 断言和多个实际 key/header/hash 核对。删除恢复针对 executable/LLVM 文件，不是删除 layout；末尾原值返回也没有被标成四槽历史必命中。这些均是已读源码中的断言，本审阅未执行。

此前 retain03（72/24/10）与 check02（102）继续绑定原 404a，不迁移为 305 的收据。gate09 状态由 root 统一收齐；本报告不宣称新正式质量或性能通过。
