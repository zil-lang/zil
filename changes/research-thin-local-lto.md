# 独立 ThinLocal 对照候选

基线ee786e7fd7162cfd76d5f532fe5c80d9e175ad57。对照实测Rust1.99.1源码及既有工作产品，确认其O2/requested-CGU8默认使用ThinLocal，持久保存pre-LTO bitcode与由真实imports/exports/liveness等计算的post-LTO键。此前仅“多CGU+手工小函数导入”不足以代表该策略。

先在研究专用源码快照中实施，不修改当前生产。快照仅给Clang路径增加当前结构化machine preflight、实际plan.units输入/导出清单收据，并使单CGU也从实际unit-0.ll进入clang。可关闭手工正文导入，仍保持固定owner分区，单独观察ThinLTO作用。生产代码、默认CLI、语言Wire、默认预算与独立Verify均不修改。

研究wrapper仅接受快照捕获的实际unit-N.ll，并逐字节匹配其当前预检SHA；公开program.ll不作为分区输入。基线和ThinLTO都使用同一个预检快照、同一clang/lld与O2。前者生成普通对象，后者生成thin bitcode并用lld ThinLTO链接。默认模式保留原External定义的导出根；显式closed只允许当前LIR生成函数内部化，foreign/入口/非候选外部定义保留。函数地址必须保持唯一。

前阶段本地可信缓存键包含完整规范化unit文本、编译器/clang/lld身份、优化与策略参数；只消除具名verified-fingerprint诊断注释，当前验证授权另记。ThinLTO后阶段复用由真实lld缓存机制负责，不用本单元SHA替代LLVM的imports/exports等键。每请求总是重新链接；全链接输入/命令和实际输出SHA记录，不声称已经实现完整链接动作缓存。前后阶段新缓存均先留在请求私有目录，只有外层driver Success才发布。损坏前缓存视miss，不借缓存构造Verified。

工具需保留快照修改diff与原/后SHA、当前Verify/preflight收据、每次原命令/替换命令/真实IR与bitcode、导出根、工具身份及native结果。失败、重复符号、地址、清理和冷暖/私有修改的失效控制在功能窗口验证。端到端计时必须包括新增预检、文本/bitcode生成、链接与收据开销，只有同快照Clang对Clang可以隔离ThinLTO效果；API与文本后端差异不得归为LTO收益。

此候选不是当前生产实现，也不预设更快。先测实际运行质量/代码尺寸和编译时间，尤其高扇出与opaque-input大模块，再决定是否形成原生API bitcode/summary/pre-post键的生产接口。hidden/dso_local与内部化有不同语义；不得为了速度直接删默认导出合同。

独立附加对照为harness --dso-local：按结构化当前External函数定义清单添加LLVM文本属性，AE导入/extern声明不动，原/后文本与选项进入证据及key。当前LLVM23 C API没有LLVMSetDSOLocal，因此此片只测Clang文本路径，不扩大C++ FFI，也不宣称API已实现。默认外部导出根不删除，不能把该属性混同hidden或internal。

功能控制发现第一epoch没有启用section GC：ThinLTO虽然消除了调用，canonical owner正文仍留在最终可执行文件。第二epoch对两臂统一增加编译-ffunction-sections/-fdata-sections与链接--gc-sections；默认导出根照旧保留。该差异必须单独记录，不能把链接策略变化冒充ThinLTO收益。Rust精确revision的back/link.rs在!link_dead_code时调用gc_sections，来源与本地实际bitcode链接控制一并保留。
