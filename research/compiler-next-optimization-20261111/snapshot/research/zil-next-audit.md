# main990：保持完整优化可见性的机械去冗余审计

## 结论与基线

建议先单独实施 **普通函数发射直接写结构 sink**，再把 **LLVM builder 基本块定位复用**作为独立小提交审查。两者都删除当前路径中的重复构造操作，不改变分区、导入集合、符号身份、函数体、检查、O2、copy fuel 或语言预算。这里只证明有可删工作，尚无新性能结果；尤其不能声称解决 root CGU 完整导入后的重复 LLVM 优化。

只读基线：`99035931d358add0eaed6d4535035c38413701dd`，tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`。已核本地 checkout/main 和干净状态，读取当前 AGENTS、远端 goal:goal.md（blob `e45b46d5c6bac14491c396f9f22e39134b2d2410`）与 goal:workflow.md（blob `b19e65565e8554a6f2c6dcb08760727d2e7ff16d`）。本次未修改生产源码、未构建、未跑测试或采样。

## 候选一：普通函数直接写最终机器结构

### 当前重复工作

`crates/zil-backend-llvm/src/emit.rs:643–674` 为每个普通函数创建 `m::Code(Vec<Event>)`，`emit_with_cleanup` 完成后再调用 `Code::into_items`。`crates/zil-llvm-api/src/model.rs:700–745` 随后第二次遍历事件，把每个 instruction 从事件数组搬到 `Block.instructions`，把 block/function 搬到最终 items。当前已经按函数释放 tape，不能把“whole-module tape 改为 per-function”再作为新优化；本候选删除的是仍存在的 **per-function tape**。

foreign、constant summary、incremental scalar-leaf 快路径分别在 emit.rs:584、603、607 附近直接或独立生产机器项，绕过普通 FunctionEmitter；这些路径不会受益。冷请求、编辑请求和对象全命中的暖请求都会执行当前机器 lowering，因此只要走普通 emitter，本候选都可能省工作；不能说每种请求都必然更快。

### 最小改法与精确写集

建议把 sink 保持为后端私有类型，不扩公共 model API：

| 文件 | 改动 |
| --- | --- |
| `crates/zil-backend-llvm/src/emit.rs` | 增加私有 sink 子模块；FunctionEmitter.text 改为 sink；普通 emit 返回 `Vec<Item>`，删除其后 `.into_items()`。保留 `event`/`line` 的既有处理层级。 |
| `crates/zil-backend-llvm/src/emit/items.rs`（新） | `items + active Function + first_error`；暴露 `push(Event)`、`append(Code)`、`finish()`。同现有 into_items 状态机消费事件，但立即移入最终 block。紧凑差分测试同文件。 |
| `crates/zil-backend-llvm/src/emit/scoped_thread_tests.rs` | 两处 raw-emitter 构造/合并适配新返回值；原 Source/native/assertions 不删。 |
| `crates/zil-backend-llvm/src/emit/scoped_callback_tests.rs` | 同上，声明 prelude 先转 items，再接 raw-emitter items。 |

预计约 80–130 行生产实现/接线，另约 40–70 行紧凑测试/测试接线；这只是静态估计。`copy_aggregates.rs` 和 `dyn_dispatch.rs` 现有直接 `self.text.push` 可以保留，因为 sink 保持 push 接口，不需要批量重写全部 opcode。

### 必须保持的顺序与拒绝合同

1. `emit.rs:3624` 的 `event` 会调用 `copy_event`；`emit/copy_aggregates.rs:938–970` 先检查 large-Copy snapshot 参数，发现被禁止的结果时设置 `unsupported_large_copy` 并抑制该事件。sink 必须在这层之后，不能绕开它。
2. `line(Code)` 当前只 `self.text.append(code)`，并未逐事件调用 copy_event。新 append 只能把这些事件送入结构 sink，不能额外改变 Copy 检查路径。
3. 结构错误目前在完整 `emit_with_cleanup` 成功返回后，才由 into_items 报出；其中 `unsupported_large_copy` 在 emit.rs:1793–1799 先报。新 push 必须记录第一个结构错误，之后忽略结构拼装但仍让 emitter 执行原有工作，到 finish 才返回同一错误。不能 push 时立刻提前返回，否则会改变后续语义错误与结构错误的优先级。
4. Begin/Label/Instruction/End、declaration/global/named type 的合法位置、重复 Begin、缺 End 等消息与 first-error 必须相同；函数主定义之后出现的 helper 定义仍按原事件顺序排列。
5. scalar_locals::promote、task machine 收集、源函数顺序、并行 batch/join、预算扣费及取消边界不移动。

### 工作量与收益上界

设普通函数产生 E 个事件、I 条机器指令：省掉 E 槽 Event 暂存、对应 Vec 扩容及第二遍 E 次 enum dispatch/move；最终 I 条 instruction、字符串和嵌套操作数仍完整拥有，原机器验证与 LLVM 工作不减少。原操作是 move，不是逐指令深 clone，不能把“删除第二遍”说成省两份 operand 分配。峰值改善主要来自临时 Event 容量与最终结构并存；最终 block Vec 的扩容仍存在。

旧 profile 的 machine lowering 104.70ms 是此候选的粗上界，远非承诺可省 104ms；Verify、checked/scalar fold 规划、布局、worker、LLVM O2/emit 和最终对象析构没有被消除。sink 每次 push 的状态分支可能抵消一部分收益，需要完整请求 screen 裁决。

### 必要验证与退出条件

以现有 `Code::into_items` 作为独立参考：同一紧凑事件 fixture 比较完整 `Vec<Item>` 或完全相同的错误，覆盖主函数+helper、多 block、所有顶层项、四类结构错误、带后续事件的 first-error。保留现有 raw scoped-thread/callback 真实测试；后端普通 checked arithmetic、owned cleanup、大 Copy、task/resource 测试应继续走新 sink。原机器模块/完整 sidecar 必须相等，再核完整 Verify、每 unit preflight、O2 与原生输出。若错误顺序需要大范围 API 改动、额外 opcode 镜像或全局模式开关才能保住，停止此切口；若固定完整请求无净益，不因局部 allocation 下降采用。

## 候选二：LLVM builder 复用当前插入块

### 当前重复工作

`crates/zil-llvm-api/src/native/builder.rs:650–690` 创建基本块后，先预建 Phi；随后每一条非 Phi schedule 指令都执行 `self.blocks[blocks[b].name.as_str()]` 的 BTreeMap 查找和 `LLVMPositionBuilderAtEnd`，即相邻指令属于同一个 block 也重复。完整搜索该文件，位置设置只有 Phi 预建的第 671 行与普通 schedule 的第 687 行；`emit` 的 opcode arms 没有重设插入点。

### 最小改法

只改 `native/builder.rs`：创建 block 时同时按源 blocks 顺序保存 `Vec<Handle>`；schedule 的 `(b,j)` 直接索引该 Vec，保留 name map 供 branch/Phi 前驱解析；在普通 schedule 循环使用 `last_block: Option<usize>`，仅 b 改变时 PositionBuilderAtEnd。在 Phi 预建结束后设 None，让普通 schedule 第一步总是明确定位，避免依赖 Phi 阶段最终 builder 状态。保留同一个 schedule、同一个每 256 step 取消检查和同一个 emit 调用顺序。预计约 10–25 行净生产变更，独立于候选一。

不重新分组 schedule！`preflight.rs:998–1067` 当前按 SSA 和块内顺序边做 FIFO 拓扑排序，可能交错多个 block。设非 Phi 指令数 I，schedule 的连续同块 run 数为 S，准确减少 I−S 次定位 FFI；最坏 S=I，定位收益为零。dense handle Vec 仍把 I 次块名树查换成索引，但新增 B 个句柄的 request-local 容量；无需 new cache、persistent receipt 或 compiler-domain 变更。

### 质量不变的依据与风险

在 current-block 仍相同且没有别的 API 改变 builder 插入点时，再次 PositionBuilderAtEnd 等价于保持原插入点。所有 LLVMBuild 操作、参数/属性、块/函数名字与创建顺序、Phi 预建及最后 backedge 填充均相同。LLVMBuild 常量折叠可能不插入 instruction，但也不移动插入点；这不要求关闭折叠。不能把 last_block 跨函数共享；不能用“每个 block 只定位一次”替换，因为 schedule 会返回之前的块。

必要控制已有 `structured_llvm_builder.rs::direct_loop_forward_block_phi_executes_o0_o2`、`cancellation_at_each_native_boundary`、`machine_preflight_and_builder.rs` 的前向块/非法 SSA/Phi 控制，以及 `llvm_builder_allocation_cleanup.rs`。只需要额外一个确实交错 block 的紧凑机器 fixture，确保返回既有块时定位、同块连续链和常量折叠同时正确；保留 reference 路径比完整 IR/对象/输出。未来新增会移动 insertion point 的 LLVM API 必须同步使缓存失效；可在该循环旁注释此局部不变量。

收益只覆盖 **object miss 的 builder_ns**（native.rs:336–350 单独计时），对 object 全命中请求没有直接作用，也不减少 LLVM optimizer/机器码 emit。应先看已有/下一次正式 request receipt 的 builder_ns 占比，不能为毫秒以下相位无限扩测，更不能称解决 1679 所暴露的 root import 放大。

## 其余方向为何暂不列为新候选

当前 `machine_cgu.rs:184–218` 已有精确输入 layout 缓存；`layout.rs:42–115` 已把最后一个消费者的 body 用 distribute_item 移走，只在多个消费者确需拥有时 clone，import 仍转 Internal 私有实现并仅重写直调。把名称/引用投影再缓存、改变所有权分组、稳定 owner、缩减 body 或以 available_externally 免 emit，都可能改变 IPO/身份/运行质量，或已经有本项目否定证据，不能冒称新增冗余消除。immutable/shared body 表示需要穿过 rewrite、fold、preflight、缓存与LLVM边界，不是最小施工切口。

Rust/Blaze 值得借鉴的是明确阶段拥有关系与只保留实际需要的工作产品，但不能从另一实现的 per-function artifact、增量图或 CGU 策略直接推出本项目等价。这里先采用更窄的可证明机制：去掉生产者到消费者之间暂存的第二份容器、去掉已知状态未变时的重复 FFI；不为移植而换 IR/缓存架构。

## 历史范围与未测边界

已复核 goal 当前 G10.5.1.2/.12/.13/.14 和精确历史 `8255079de3434de5aad44805a12eed8e9f433c24` 的 `changes/research-direct-machine-text.md`、`changes/research-work-aware-parallel-lowering.md`：前者是保持 sidecar 字节的直接 String renderer，后者是已选 scalar-leaf 计划与工作量感知并行调度，均不同于本报告的 per-function Event sink 或 builder block-position 复用。当前 checkout 为 shallow main 单提交；没有据此宣称查尽全部历史。

上轮明确否定的 layout 投影（总体假设加权 0.9475%、12 快 12 慢）、bounded imports（运行 +61.6%–112.3%）、完整 AE 四臂、CP084/反向索引、单次构图回退复用、worker-owned unit 析构和前置合并仍是排除项，本轮不复刻、不改名重试。两新候选都尚未实作、编译或计时；源码等价论证不代替最终 runtime ≥ Rust 90% 与相对不退化 10% 及正式加权门。

## 后续已授权实施快照（与上述只读评估分开）

Root随后授权两个独立候选，均从精确main990起步，未互相叠加：

| 候选 | 本地源码提交 | tree | 工作树 | 实际写集 |
| --- | --- | --- | --- | --- |
| 直接结构sink | `924076ca11c75e88bb1bcb9df98c58d0dae1bbd2` | `ad37913cf49a1bfec28e66e7abdcf225dbcafed5` | `/workspace/scratch/abf690ce880c/structured-emission` | `emit.rs`、`emit/scoped_callback_tests.rs`、`emit/scoped_thread_tests.rs`、`zil-llvm-api/src/model.rs`、新增`zil-llvm-api/src/item_builder_tests.rs` |
| builder定位复用 | `78e300443e162bbb963b809e1066b9bd08197a77` | `257a29612531576af3f9fcf1f703fb824970f695` | `/workspace/scratch/abf690ce880c/builder-position` | `zil-llvm-api/src/native/builder.rs`、`zil-llvm-api/tests/structured_llvm_builder.rs` |

第一候选实际采用Root明确批准的限定公共接口调整：在model增加ItemBuilder并让Code.into_items共用状态机，保持Code公开tuple兼容。原先报告建议的后端私有sink文件没有新增。大Copy优先级反例并非直接置flag：先注入结构错误，再经过真实copy_event检查一个分类后的物理snapshot参数，要求最终仍报原Unsupported诊断；不带snapshot时才报原结构诊断。

第二候选仅增加局部block_handles与current_block，未改变schedule生成/顺序或跨函数复用状态。新增fixture包含起始即ready且会被LLVM折成常量的right-block表达式，以及entry/left/right实际链，要求O0/O2均运行返回42。

作者本次仅执行rustfmt（Rust语法格式化）与git diff --check；两个候选均未由作者构建、测试、运行或计时，未作远端mutation。构建、非作者精确head独审、原生功能及固定完整请求性能/运行质量由Root统一安排。该研究报告留在research目录，未混入两条生产源码commit。
