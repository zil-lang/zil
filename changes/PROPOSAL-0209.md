# 209 主线整合与研发默认LLVM API

依据：用户明确要求当前版本集成、同步GitHub、LLVM API成为主路线，并优先终态架构。远端冻结在ab2aaa7aa25e3c10d2908801d5e06dff1079df25；本地输入208为0f391c0a8cb8a421ffca5cef441fee201cf7c123。

先做源码三方合并，保留远端CI与字节截断实现；不导入8100项历史证据。当前crates与208逐字相同。

策略变更：标准zil-cli构建启用llvm-api；CLI选择默认由该已编译特性决定。显式--backend clang继续作为差分路径；--no-default-features为不链接LLVM的精简对照构建，并在显式请求API时拒绝，而非静默回退。底层库CompileOptions默认策略保持Clang，库调用者显式给定策略，避免特性合并改变嵌入者的选择。

这是用户授权的研发默认策略变更，不是产品性能默认门通过；规范19.5相应区分研发入口与产品默认推荐。既有数值验收门、语言接受集、Verify、预算、O0/O2语义不变。默认CGU仍为1；API仍是文本解析阶段，直接Builder/稳定身份/结构化CGU/对象缓存未完成。

验收：新默认及显式Clang源码到原生；无特性拒绝；原workspace两配置；现有177消费者及失败注入；实际GitHub基线在bundle恢复后快进整合。GitHub写入必须实际推送并回读，工具不可写时交付可推送bundle，不能记为已同步。
