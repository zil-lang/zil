# Blaze v2 对 ZIL 编译架构的启示

参考固定为 indexable-inc/blaze@a2b689f24c3bca40f4d02a949cd9e5b0adac9e12，核对日期2026-11-10。README明确把已公布提速数字归于v1，v2尚未同口径测量；docs/v2/plan及compile-time/memory文档带plan标记。下面区分已阅读的实际机制、设计目标和ZIL的实施范围，不把别人的目标写成自己的实测。

## 结论

采用“常驻且有界的语义状态＋精确依赖失效＋分层代码生成”的方向，但不整套复制Blaze。ZIL应把开发反馈时间与发布程序运行质量分轨：开发档可用Cranelift，发布档继续LLVM；二者共享已经核验的所有权、布局、ABI及清理语义。先消除可证明的重复工作，再引入新后端；否则只会把相同膨胀IR更快交给另一个优化器。

## 实际源码中值得吸收的四层身份

`crates/blaze-query/src/lib.rs` 的结果按QueryKind和内容参数命名，memo保留按实际读取顺序排列的依赖边及指纹；核验在第一条变化边停止，防止继续查询新程序已经不存在的依赖。值被逐出后，指纹仍可留下，维持结果未变时的传播截断。多个不可变view共享相同内容结果，不以某个分支号把全部缓存割裂。

`crates/blaze-codegen/src/backend.rs` 进一步区分具体函数的code、可复用的template_code、实际引用绑定、被内联callee的built_in_code、跨版本identity以及调试span anchors。相同机器模板并不代表相同函数实例。可由外部命名或比较的地址不能随意合并，依赖图也必须记录实际内联的callee版本。

`crates/blaze-codegen/src/content.rs` 的代码身份覆盖架构、对齐、机器码、重定位、关联数据、展开信息、行表和覆盖/性能计数。可写数据另外混入owner和index，避免两个拥有者的相同初值变成同一个可写对象。ZIL尤其需要保留捕获环境、drop布局、资源身份和导出ABI，不能仅按闭包源码文本去重。

因此ZIL的缓存键应分别绑定：语法/相对位置；名字解析与可见性，包括“未找到候选”等负依赖；类型/trait/provider/生命周期与捕获布局；实际机器代码输入与导入优化体。名字重排、添加impl、捕获从值变借用、ABI变化都应有专门反例。只把整项目hash换成函数hash是不够的。

## 内存换时间的正确对象

优先长期保留不可变语法、名字/布局摘要、类型化函数体、依赖边与昂贵机器片段；只保留当前语义快照及有界历史，而不是每次编辑复制整个图。临时构造、哈希和lowering缓存按worker复用；共享值先驻留一次再分发引用。逐出结果值时可保留较小指纹，只有真实请求才重建值。

Blaze memory设计区分task/request/view/image生命周期，并按子系统统计live/high-water。其文档同时记载较大常驻内存实例，说明“用内存”不等于“无限缓存”。ZIL必须测冷启动、热编辑、无修改、重启、逐出后重建、长时间编辑和多项目竞争，并报告RSS/峰值/逐出次数及每秒等待节省的内存代价。可选内存档位的阈值须由测量决定，不预先承诺固定GB下达到某速度。

不能把缓存命中视为权限凭证。当前Analyze/独立Verify合同保持；任何将Verify本身增量化的工作必须独立记录并验证完整依赖与当前义务，不能拿旧授权直接复用。远端内容哈希只检验字节一致，不证明返回结果的语义可信。

## Cranelift如何落地

第一步应是显式开发后端，而不是替换发布后端。前端/Verify之后保留同一份后端无关lowered表示，包含明确布局、调用约定、捕获环境、drop/失败路径和溢出行为，再分别交给Cranelift与LLVM。不得通过解析LLVM文本绕接，也不得在未支持的操作上猜测生成代码；缺口返回具名Unsupported。

初次闭环必须包含真实捕获闭包、动态缓冲区、直接/间接调用、返回ABI、分配失败、清理顺序和溢出，而非只展示整数加法。开发档分别比较LLVM O0和Rust开发配置；发布档仍比较LLVM O2及Rust O2。测试反馈指标是编译加运行总时间，不能只报更快生成而忽略测试运行明显变慢。跨线程、FFI和不同平台再独立扩展。

copy-and-patch、按需机器码、自动分层JIT、OSR和无链接热更新具有更高工程成本。Blaze文档的相关目标不代表已具备全场景验收；ZIL先做可执行文件/AOT的Cranelift闭环。以后热更新还需处理旧栈帧、类型布局变化、析构责任、函数指针及正在运行线程，不能当普通缓存替换。

## 本轮已做与未做

已合入PR75：Analyze资源初始化顺序的精确稀疏字表示，默认限额下owned64由拒绝变为成功；既有八样本整体加权改善仅0.05%–0.22%，不作为大幅加速宣传。

已写入task/G10.5.1.6并验证中：按“精确清理动作、LocalId、后继”驻留函数内相同退出后缀，保留原live标志和析构顺序。这借鉴内容共享，减少重复LLVM输入，不引入noinline。不是常驻服务，也不是Cranelift已接入。

仍待实现：常驻编译worker及不可变请求快照、字节级内存预算、一般资源/trait/闭包精确失效、机器片段复用和Cranelift后端。现有CompilationSession已经有部分保留能力，应扩展它而非再造第二套互不相通缓存；先观测当前阶段成本和缓存失效原因，才决定每层改造。

## 参考

- [Blaze README及版本边界](https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/README.md)
- [v2总体设计](https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/docs/v2/plan.md)
- [真实query代码](https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/crates/blaze-query/src/lib.rs)
- [真实codegen身份/边界](https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/crates/blaze-codegen/src/backend.rs)
- [真实代码与数据内容键](https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/crates/blaze-codegen/src/content.rs)
- [内存目标及实测观察](https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/docs/v2/plan/design/memory.md)
- [分层编译计划](https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/docs/v2/plan/design/compile-time.md)
- [Cargo官方编译效率与Cranelift取舍](https://doc.rust-lang.org/cargo/guide/build-performance.html)
- [Rust drop elaboration](https://rustc-dev-guide.rust-lang.org/mir/drop-elaboration.html)
- [Cranelift官方定位与后端接口](https://cranelift.dev/)
