# O2 稳定 CGU 与按实际引用导入正文

基线3d6babe17。用户要求在新大模块分布下重新实施图策略，并重点缩小Rust O2差距。此处研究的是机器符号引用图分区，不把旧076资源联合关系图切割的结论直接套用到CGU图。

现有O2将同一源文件整体union，8192函数文件只能形成一个CGU。直接拆开会损失跨单元优化机会。因此新增显式 incremental_o2 构建策略（CLI --incremental-o2，默认false），在O2多CGU时复用固定256函数owner块，并像Rust跨CGU优化实体一样按实际引用导入小函数正文。导入采用LLVM available_externally：仍保留唯一外部定义，导入体仅供优化，不独立发射，函数地址仍指向同一规范符号。不开启额外优化级别或LTO。

只导入当前LIR非foreign定义，成本不超过64条机器指令/块，单CGU最多65536成本，按当前确定性定义序拓展实际引用闭包。引用local定义的函数不导入，以免复制静态身份。导入依赖所需声明完整加入，未被实际导入的正文不污染单位缓存键。导入的完整正文进入既有对象键和每请求机器预检；任何缓存命中仍需当前Verify。阈值只限制可选优化，不改变语言预算、接受集或默认预算。

写集：machine_cgu.rs、backend build.rs、zil-llvm-api model/preflight/native builder及对应测试；root负责driver/CLI显式选项接线。不改公共Wire/schema/Cargo.lock。AvailableExternally只扩展内部拥有式机器表示，不能用于无正文声明或全局变量；原Internal/Private身份约束保持。

验证包括：跨块正文命中/失效、传递导入及只读声明边界、相同函数地址、local初始化/资源清理/闭包、完整API/text-reference和真实源码原生控制；O2原版本与新版本都保留。完整冷/暖/前后部编辑时间、内存、native行为、运行时间与代码尺寸分别测量。导入不是免费：高fanout可能使入口CGU消费很多正文，修改仍使该CGU失效；必须测全成本，不能只报告cache hit或分区数量。是否默认晋升由完整实测决定，未证明前不改变默认策略。
