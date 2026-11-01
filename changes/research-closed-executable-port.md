# 复用CP13的显式封闭可执行构建

基线5189f0975。旧research/compile-performance e7b492c6已有CP13内部化候选；本片明确移植其已审过的机制，不称原创。新大模块诊断再次观察到O2在优化和发射所有External的zil.fn定义上付出大量成本，而Rust的私有函数可被内部化/消除。

新增显式 --closed-executable，仅Linux完整build可用，默认false。用户启用即声明生成zil.fn身份符号没有外部导出合同。保持普通外部C声明、main/wrapper、全局数据及函数地址语义。分区后只内部化精确当前非foreign LIR函数白名单，任何其他CGU实际引用的函数保留External；重复定义回原机器预检。O2管线和前后验证不变，emit接口与默认build不变，单CGU clang也必须消费变换后的实际unit。

写集追加CLI main.rs及CLI/driver/backend限定测试、BuildOptions/CompileOptions的默认false字段、session的build传递、machine_cgu内部策略。公共Wire/schema与现有JSON报告形状不变；作用由显式命令、实际LLVM/linkage/nm控制和对象身份验证。对象键本就绑定linkage及编译器ELF，不增加新缓存授权。

同一编译器default/closed/closed/default对象缓存序列、O0/O2、1/8CGU、API/clang、源资源/闭包/地址、伪造LIR/Win64/链接失败/旧输出保留均需验证。Rust比较分别报告默认与显式closed，不能静默把更窄的导出合同称为默认收益；未实现全程序LTO或跨单元内部化。
