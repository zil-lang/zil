# 181：Win64 Store 的平台合同

## 为什么

177 的 Store 运行时使用既有字节分配适配层，但 Verify 和 Store emitter 仍只准入 Linux。
ZUI 的 Win64 状态容器必须经过真实目标构建与执行，不能仅修改支持矩阵。

## 是什么

只开放现有 `TargetProfile::is_supported_x64_llvm()` 精确准入的 Linux x64 / Win64 MSVC。
Store 元素、头布局、初始化前缀、检查溢出、push失败退还、pop和Drop顺序保持同一合同。
Win64 复用已经存在的 `GetProcessHeap / HeapAlloc / HeapReAlloc / HeapFree` 适配；
无CRT allocator混用，不把未知目标退回宿主。封闭元素最大自然对齐不超过8。

Microsoft 的 HeapAlloc/HeapReAlloc 文档规定 Win64 对齐16，失败返回NULL且
HeapReAlloc失败保持旧块；flags保持0，不使用HEAP_NO_SERIALIZE或异常生成。
来源：https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapalloc
和 https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heaprealloc。

## 怎么做与验收

Verify从目标profile独立准入；后端同样检查精确目标，未知目标保持Unsupported。
O0/O2对完整Package参数/错误返回/Store字段/清理生成PE，使用显式import库。
同时覆盖无ByteBuffer字段的Store（仍须生成allocator）、不同padding/stride元素、
满push退还、pop及剩余前缀Drop、reserve失败保持、越界trap和真实分配故障。
原共享窗口/opaque回调/固定资源规则不放宽。

Wine执行只登记兼容层证据；原生Windows未执行仍是NotRun。
测试侧可通过显式独立heap shim注入失败并计数，生产ZIL程序不依赖该shim。
未完成真实执行前不登记目标通过，不改变任何产品预算。
