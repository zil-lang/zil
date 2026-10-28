# 212：核验之后复用原生对象

## 为什么不是再优化一层LLVM调用
211已经直接使用LLVMBuilder，但每个CLI请求仍构造、优化、发射全部对象。
本片不改变相同机器输入的优化结果，而是在当前核验完成后复用已成功生成的对象。
只减少已完成的重复工作；冷构建仍需新增身份识别、键计算及缓存写入成本。

借鉴Rust的代码生成工作产物复用，以及Go的ActionID/OutputID区分；见REFERENCES.md。
没有移植外部源码正文，也没有用Rust借用检查器替代ZIL规则。

## 三个不能混用的身份
1. 当前验证授权：由当前请求独立Verify产生，绑定当前源码/规则/目标。
2. 对象动作键：当前结构化CGU全部有序机器输入、实际编译器及LLVM安装身份、目标和优化配方。
3. 对象内容摘要：SHA256，仅用于检测条目损坏，不证明对象与LIR语义一致。

动作键不包含仅供诊断渲染的全局验证指纹注释；Builder从未读取该注释。
当前授权仍必须重新取得，机器预检也仍执行。没有反序列化Verified的入口。
编译器身份绑定Hash实现及固定CPU/features/data-layout生成、pipeline、重定位和代码模型。
不排序指令/字段/捕获/析构/符号顺序，不按字符串删去可能影响语义的数据。

LLVM身份通过实际加载的LLVMGetVersion符号定位动态库，而不是猜测环境变量路径。
可信、执行期间不变的安装文件采用GNU build-id加文件元数据；无build-id时流式SHA256。
同一版本号不视为同一工具；安装位置或文件元数据变化允许保守miss。
这个协议不是恶意二进制鉴真；安装文件及本机缓存目录属于显式供应链可信边界。
LD_PRELOAD/LD_AUDIT非空时可见地禁用缓存，避免在不完整的动态插桩身份下命中。

## 请求生命周期
```
源码 → Types/Analyze → 当前独立Verify → Lower → 结构化机器IR/CGU
  → 计算动作键
  → 命中：核对内容 → 当前机器预检 → 暂存对象
    缺失/损坏：原直接Builder、LLVM前后核验、优化、对象发射 → 暂存对象
  → 链接 → 转储与发布检查 → 最终成功发布
  → 最佳努力原子写入新缓存条目
```

失败/取消时，未发布对象只存在当前请求内存和暂存目录，不写新缓存；旧成功产物保护不变。
最终发布后的缓存写入失败仅计write_failures，不把成功可执行文件伪装为失败。
缓存损坏、截断、不匹配、不可读时按miss重新生成；根目录不适用时可见bypass。
缓存文件单条有长度上限128MiB，无总量淘汰策略；用户可以删除整个缓存目录。

## 使用
```
zil build main.zil -o out/app --opt O0 \
  --codegen-units 8 --codegen-jobs 4 \
  --cache-dir .zil/source-candidates --object-cache .zil/native-objects
```
两种缓存不能混同：`--cache-dir`仍只保存未核候选，不自动开启对象复用。
仅`--object-cache`显式开启；不传时沿旧路径生成对象。当前仅Linux宿主支持本地对象缓存。
`--backend clang --object-cache DIR`明确拒绝，不冒称Clang完成缓存读取。
默认单单元/单工人未改变。O2多单元此前的运行性能风险不因缓存而消失。

JSON的native_codegen.object_cache报告enabled/hits/misses/corrupt/stored/write_failures及键/读取/预检计时。
命中的object_reports.input_kind是object-cache，llvm_verification_this_request=false；
它只声明原生产者完成LLVM前后核验，不假装本请求重新运行这些pass。
verification_runs仍必须为1，真正的外部命令仍只有实际链接等操作。

## 已知边界
- 每次仍做当前Verify、Lower、机器IR、诊断.ll输出及链接；不是构建系统no-op。
- 稳定跨请求声明/实例身份尚未实现：新增无关声明仍可能扰动许多单元。
- 单元划分仍来自210/211，不声称任意编辑仅重建一个函数。
- 缓存仅用于明确可信的本机目录和不可变工具安装，不支持不可信/共享网络缓存。
- 文件SHA不能保护同时篡改头部和对象的攻击者；0700/0600、拒绝符号链接及可写共享目录
  是减少误用，不宣称对恶意同用户进程提供隔离。
- 没有降低逻辑配额、语言规则、失败/溢出/析构语义或产品验收条件。
- 缓存命中不证明LLVM/后端本身正确；它们仍属于原已声明可信边界。
