# 私有导入插件微控制源码

仅控制源码，作者未编译、未加载LLVM、未运行native。此目录位于正式script ROOT外，不参与正式lld加载；PrivateImportsControls.cpp以NO_ENTRY宏包含精确正式PrivateImports.cpp，复用inspect/rewrite，另建独立test-only opt插件。正式插件只有ThinLTOPostLink O2 EP入口，不注册本控制的手动pipeline。

root可在授权窗口内分别构建两份shared plugin，均用官方23.1.4 headers、C++17/ABI1/-fno-exceptions/-fno-rtti，LLVM符号由各自host解析；不静态链接整份LLVM、不-lLLVM，Linux系统-ldl允许。正式插件加载ld.lld；控制SO仅加载官方opt。

控制通过后也不能冒充正式manifest、自身SO绑定、八backend收据或runtime质量验收；控制报告显式formal_manifest_receipt_path_exercised=false。

两个输入均先由实际LLVM parse+verify接受：

1. 唯一直调callee：保持Function对象、type/attrs/body及CallInst目标指针，只改名字为zil.import.0.callee和Internal；旧名不留declaration或definition，后LLVM verify。
2. 同一个CallInst同时call @callee(ptr @callee)：callee Use和参数Use共用一个User，必须按Use角色拒绝；函数User是CallInst本身不足以放行。检查模块全文未变并再次verify。

可用的后续控制命令形状（本次没有执行）：opt --load-pass-plugin=ABS/PrivateImportsControls.so -passes=zil-private-imports-controls -disable-output EMPTY.ll。是否host导出全部控制所需符号由root加载时验证，首败原样保存，不改用第二份LLVM绕过。正式CPP和控制CPP文件SHA必须与各自构建receipt绑定。
