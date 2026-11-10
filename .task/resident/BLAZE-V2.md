# Blaze v2 对 ZIL 编译架构的启发

固定参考 `indexable-inc/blaze@a2b689f24c3bca40f4d02a949cd9e5b0adac9e12`。当前README把公开性能数字归于v1，旧architecture.md标注superseded；v2计划、真实源码和测量不能互相代替。

## 结论与本轮落点

选定方向是：常驻、精确依赖的前端 + 可重绑定的函数代码模板 + Cranelift开发档 + LLVM发布档。内存用于保留已算过的结果，不能只扩大缓存却保留整项目失效，也不能为编译速度禁用所有闭包内联。先测已有CompilationSession保留收益、减少当前重复清理，再建立通用函数片段合同。

本分支已实现并测量标准输入协议的常驻CompilationSession原型，结果见RESULTS.md；清理后缀内容驻留在独立task/G10.5.1.6验证。一般精确语义增量、Cranelift、JIT、自研链接器和后台热点升层仍未实现，不因采用路线而获得验收。

## 精确依赖，比常驻本身更关键

实际`crates/blaze-query/src/lib.rs`按内容输入保存查询结果，memo保留按读取顺序排列的依赖与当时指纹。检查遇到首个改变就停止，避免访问旧执行分支才存在的依赖；重新计算仍得到相同指纹就截断失效传播。不同view共享相等内容，只有输入查询依赖具体view。[B1]

ZIL应按真实读取拆开接口、闭包环境布局、调用种类、清理/借用合同和实现体。只改callee函数体，不应重编没有内联它的普通调用者；改变捕获布局或实际内联体必须影响真正消费者。trait实现集合变化和歧义也必须入依赖，不能只缓存已选中的方法。已合标量签名依赖只是有限切片。

## 代码模板，不能预先嵌入旧调用目标

实际`Template`保存代码、数据、unwind、行号及内联依赖；普通函数/静态/外部符号的重定位按引用列表槽位表示，在本次构建再填当前目标。[B2] 若callee只改实现，调用者模板可不变，绑定表改变即可。只从key删除callee内容却在缓存value保留旧callee身份会错误命中，不是合法优化。

发布档不同：实际导入/内联的函数体属于优化产物依赖。Blaze的release summaries/imports和`Template.inlined`保留这些信息，不是所有调用永远只依赖签名。[B3] 源位置、unwind、目标ABI、溢出检查、闭包布局和常量都需明确绑定。地址有意义的函数、静态对象和计数器不能因模板相等就合并运行时身份。

可用于后续ZIL接口的草图是`PreparedFunction { shape, interface_facts, bindings, imported_bodies, provenance }`和`CodeTemplate { code, reloc_slots, data, unwind, debug_anchors }`。这只是设计，不是已发布公共结构。

## 内存换时间的生命周期

Blaze内存方案区分任务临时、请求、不可变view和长期结果；按用途统计，读者结束后回收旧状态，避免每轮重新分配。[B4] 对ZIL先复用现有查询系统、IR和scratch，不另造平行数据库。生产服务的字节预算要覆盖嵌套IR/实例表/查询边/后端上下文/机器码/暂存峰值，而不是只数32个顶层缓存条目。

每次测量同时记录冷启动、真实编辑、暖请求、错误后恢复、清空/重启/淘汰、峰值RSS和结束RSS。原型六次请求一个worker不能证明长期稳定。不可变内容共享只计一次实际物理数据，但每个活跃请求还需要新结果的峰值预留；不能等到OOM后再淘汰。缓存校验和只说明字节身份，不是安全授权。

## Cranelift首个可验收闭环

开发档接Cranelift，发布档保留LLVM O2，两档共享前端、布局、ABI和清理合同。入口是当前独立Verify之后明确的低层表示，直接适配，不解析打印的LLVM文本。首批先AOT对象和正常链接，不同时引入JIT、热替换、惰性类型检查和自研链接器。

必须覆盖真实拥有闭包的环境创建/捕获、可变和消费调用、失败返回、清理顺序、分配失败与输出；标量加法通过不能代表闭包后端完成。未支持操作明确拒绝；任何LLVM回退要显式报告并单独计数，不能混入Cranelift成绩。跨后端混合函数需要相同ABI，不能在一个后端偷偷扁平化聚合而继续与另一端交互。

Cranelift历史增量机制把IR分为编译依赖的stencil与后置修补的参数，函数重定位和可独立修补的源位置不破坏模板。这与Blaze引用槽思路相通。[C2] Rust官方2026年cg_clif目标页也提出常驻函数机器码缓存、JIT等，文中2026年2月16核Zed端到端增量收益5%/12%，后端2倍是目标而非已完成成果。[C1] 不能据后端微基准推算整个编译会同步提速。

## 后续顺序

先减少真实重复工作并测常驻时间/内存；再做闭包/泛型的精确依赖与可绑定模板；随后接Cranelift AOT开发档；依据链接实际占比决定是否需要依赖镜像或增量链接。Blaze v2提出的copy-and-patch、lazy codegen和热点升层是再下一层。[B5] 延迟代码生成不等于延迟安全检查；热替换还涉及旧栈帧、拥有环境、跨线程回调和取消清理。发布文件必须由固定输入/设置决定，不能受后台完成时机影响。

开发档验收端到端编辑等待与内存，发布档继续验收O2运行质量；两档数字不混用。常驻实验包含IPC及响应解析，CLI对照计到进程退出，前者额外开销已说明；原型没有跳过当前Verify，没有新的Rust对照，也没有完成Cranelift接入。

## 固定原始来源

[B0] https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/README.md
[B1] https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/crates/blaze-query/src/lib.rs
[B2] https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/crates/blaze-codegen/src/template.rs
[B3] https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/docs/v2/plan/design/codegen.md
[B4] https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/docs/v2/plan/design/memory.md
[B5] https://github.com/indexable-inc/blaze/blob/a2b689f24c3bca40f4d02a949cd9e5b0adac9e12/docs/v2/plan/design/compile-time.md
[C1] https://goals.rust-lang.org/2026/improve-cg_clif-performance.html
[C2] https://bytecodealliance.org/articles/cranelift-progress-2022
