# ZIL 标准库参考（Standard Library Reference）

> **文档性质**：本页由 [`tools/gen_stdlib_reference.py`](../../tools/gen_stdlib_reference.py) 直接从仓库 [`lib/std/`](../../lib/std) 真实源码确定性抽取生成，**不手写不存在的 API**。修改标准库公开声明后，请运行 `python3 tools/gen_stdlib_reference.py` 更新本页，并用 `python3 tools/gen_stdlib_reference.py --check` 校验零漂移。
>
> 语言教程见 [`docs/guide/language-tour.md`](language-tour.md)；当前开发预览边界见 [`docs/guide/supported-language.md`](supported-language.md)；规范权威见 [`_zil_design.md`](../../_zil_design.md)。

## 概览统计

- 跨平台 / Linux POSIX 标准库模块：**28** 个（`lib/std/*.zil`）
- Windows 平台适配模块：**7** 个（`lib/std/windows/*.zil`）
- 公开类型（`struct` / `package` / `resource` / `enum`）：**72** 个
- 公开 Trait：**6** 个
- 公开函数与方法：**337** 个

## 模块索引

### 核心标准库（`lib/std/*.zil`）

| 分类 | 模块 | 类型 | Trait | 函数/方法 | 职责说明 |
| --- | --- | ---: | ---: | ---: | --- |
| 文本与数值 | [`std.utf8`](#std-utf8) | 2 | 0 | 4 | UTF-8 验证、标量定位与编码工具 |
| 文本与数值 | [`std.string`](#std-string) | 2 | 0 | 30 | UTF-8 拥有字符串（`String`）、查找、分割与替换 |
| 文本与数值 | [`std.int`](#std-int) | 2 | 0 | 20 | 十种整数的十进制解析与格式化 |
| 文本与数值 | [`std.float`](#std-float) | 2 | 0 | 4 | IEEE 754 `f32`/`f64` 文本解析与格式化 |
| 文本与数值 | [`std.text_edit`](#std-text_edit) | 2 | 0 | 3 | 纯文本缓冲与编辑操作原语 |
| 集合、存储与图 | [`std.store`](#std-store) | 0 | 0 | 11 | 线性拥有动态存储 `Store<T>` 封装 |
| 集合、存储与图 | [`std.collections`](#std-collections) | 5 | 0 | 12 | 通用集合与排序辅助函数 |
| 集合、存储与图 | [`std.pod_hash_map`](#std-pod_hash_map) | 3 | 0 | 10 | 可增长 POD 键值哈希表 |
| 集合、存储与图 | [`std.pod_hash_set`](#std-pod_hash_set) | 1 | 0 | 10 | 可增长 POD 哈希集合 |
| 集合、存储与图 | [`std.i64_hash_map`](#std-i64_hash_map) | 3 | 0 | 6 | `i64` 键专用哈希表 |
| 集合、存储与图 | [`std.i64_hash_set`](#std-i64_hash_set) | 1 | 0 | 6 | `i64` 键专用哈希集合 |
| 集合、存储与图 | [`std.string_map`](#std-string_map) | 4 | 0 | 7 | 字符串键哈希映射 |
| 集合、存储与图 | [`std.owning_entries`](#std-owning_entries) | 2 | 0 | 6 | 拥有型容器的条目抽象与转移载体 |
| 集合、存储与图 | [`std.owning_map`](#std-owning_map) | 1 | 1 | 9 | 拥有型有序/基础映射 |
| 集合、存储与图 | [`std.owning_hash_map`](#std-owning_hash_map) | 2 | 1 | 11 | 拥有型键值哈希表（支持 OOM 返还所有权） |
| 集合、存储与图 | [`std.owning_hash_set`](#std-owning_hash_set) | 1 | 0 | 10 | 拥有型哈希集合 |
| 集合、存储与图 | [`std.graph`](#std-graph) | 7 | 0 | 19 | 运行时尺寸拥有资源图（逻辑删除、回收与 ticket 复用） |
| 迭代与算法 | [`std.iter`](#std-iter) | 4 | 4 | 18 | Copy 迭代、`LendingIterator` 与拥有型 `Iterator`/`Map`/`Filter`/`Drain` |
| I/O、文件系统、网络与环境 | [`std.io`](#std-io) | 3 | 0 | 17 | 基础文件句柄与标准 I/O 错误类型 |
| I/O、文件系统、网络与环境 | [`std.buffered_io`](#std-buffered_io) | 3 | 0 | 11 | 固定容量缓冲读写器（`BufReader` / `BufWriter`） |
| I/O、文件系统、网络与环境 | [`std.fs`](#std-fs) | 2 | 0 | 13 | 文件读写、目录遍历、元数据与重命名 |
| I/O、文件系统、网络与环境 | [`std.path`](#std-path) | 0 | 0 | 5 | 路径拼接、分割与扩展名操作 |
| I/O、文件系统、网络与环境 | [`std.env`](#std-env) | 0 | 0 | 6 | 命令行参数与进程环境访问 |
| I/O、文件系统、网络与环境 | [`std.time`](#std-time) | 0 | 0 | 2 | 单调时钟与墙上时间查询 |
| I/O、文件系统、网络与环境 | [`std.net`](#std-net) | 3 | 0 | 13 | IPv4 TCP 监听与连接（`TcpListener` / `TcpStream`） |
| 界面与图形基础 | [`std.ui_input`](#std-ui_input) | 2 | 0 | 8 | UI 输入事件基础数据结构 |
| 界面与图形基础 | [`std.ui_layout`](#std-ui_layout) | 3 | 0 | 5 | UI 布局几何计算基础 |
| 界面与图形基础 | [`std.ui_raster`](#std-ui_raster) | 2 | 0 | 1 | UI 软件光栅化基础原语 |

### Windows 平台模块（`lib/std/windows/*.zil`）

| 模块 | 类型 | Trait | 函数/方法 | 职责说明 |
| --- | ---: | ---: | ---: | --- |
| [`std.windows.env`](#std-windows-env) | 0 | 0 | 6 | Windows 平台命令行与环境适配 |
| [`std.windows.float`](#std-windows-float) | 2 | 0 | 4 | Windows 平台浮点文本解析与格式化适配 |
| [`std.windows.fs`](#std-windows-fs) | 2 | 0 | 13 | Windows Win32 Unicode (`*W`) 文件系统适配 |
| [`std.windows.io`](#std-windows-io) | 3 | 0 | 17 | Windows 平台基础 I/O 与错误适配 |
| [`std.windows.net`](#std-windows-net) | 3 | 0 | 13 | Windows Winsock2 TCP 网络适配 |
| [`std.windows.path`](#std-windows-path) | 0 | 0 | 5 | Windows 路径分隔符与前缀处理 |
| [`std.windows.time`](#std-windows-time) | 0 | 0 | 2 | Windows 高精度时钟适配 |

## 文本与数值（Text & Numerics）

<a id="std-utf8"></a>
### `std.utf8`

UTF-8 验证、标量定位与编码工具。源码：[`lib/std/utf8.zil`](../../lib/std/utf8.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L2`](../../lib/std/utf8.zil#L2) `pub struct Bounds { pub first:i64; pub last:i64; pub count:i64; pub error:i64 }`
- [`L47`](../../lib/std/utf8.zil#L47) `pub struct Scalar { pub status:i32; pub value:i32; pub next:i64 }`

**公开函数**

- [`L28`](../../lib/std/utf8.zil#L28) `pub fn locate(bytes:&[u8], begin:i64, end:i64)->Bounds`
- [`L48`](../../lib/std/utf8.zil#L48) `pub fn decode(bytes:&[u8], at:i64)->Scalar`
- [`L64`](../../lib/std/utf8.zil#L64) `pub fn encoded_len(value:i32)->i64`
- [`L74`](../../lib/std/utf8.zil#L74) `pub fn encode(value:i32, output:&mut [u8])->i64`

<a id="std-string"></a>
### `std.string`

UTF-8 拥有字符串（`String`）、查找、分割与替换。源码：[`lib/std/string.zil`](../../lib/std/string.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L4`](../../lib/std/string.zil#L4) `pub enum Error { InvalidUtf8, Range, Capacity, Allocation, Internal, InvalidScalar }`
- [`L6`](../../lib/std/string.zil#L6) `pub package String { manages data:ByteBuffer }`

**公开函数**

- [`L8`](../../lib/std/string.zil#L8) `pub fn empty()->String`
- [`L9`](../../lib/std/string.zil#L9) `pub fn from_utf8(source:&[u8], byte_limit:i64)->Error!String`
- [`L22`](../../lib/std/string.zil#L22) `pub fn byte_len(value:&String)->i64`
- [`L23`](../../lib/std/string.zil#L23) `pub fn scalar_len(value:&String)->i64`
- [`L24`](../../lib/std/string.zil#L24) `pub fn capacity(value:&String)->i64`
- [`L25`](../../lib/std/string.zil#L25) `pub fn as_bytes(value:&String)->&[u8]`
- [`L26`](../../lib/std/string.zil#L26) `pub fn into_bytes(value:String)->ByteBuffer contract { result(give) }`
- [`L27`](../../lib/std/string.zil#L27) `pub fn clone(value:&String, byte_limit:i64)->Error!String`
- [`L30`](../../lib/std/string.zil#L30) `pub fn replace(value:&mut String, begin:i64, end:i64, insertion:&[u8], byte_limit:i64)->Error!void`
- [`L39`](../../lib/std/string.zil#L39) `pub fn append(value:&mut String, insertion:&[u8], byte_limit:i64)->Error!void`
- [`L43`](../../lib/std/string.zil#L43) `pub fn clear(value:&mut String)`
- [`L44`](../../lib/std/string.zil#L44) `pub fn scalar_at(value:&String, byte_offset:i64)->utf8.Scalar`
- [`L46`](../../lib/std/string.zil#L46) `pub fn scalar_at_index(value:&String,index:i64)->Error!i32`
- [`L57`](../../lib/std/string.zil#L57) `pub fn is_empty(value:&String)->bool`
- [`L60`](../../lib/std/string.zil#L60) `pub fn compare(left:&String, right:&String)->i32`
- [`L77`](../../lib/std/string.zil#L77) `pub fn find_from(value:&String, needle:&[u8], start:i64)->Error!i64`
- [`L99`](../../lib/std/string.zil#L99) `pub fn find(value:&String, needle:&[u8])->Error!i64`
- [`L102`](../../lib/std/string.zil#L102) `pub fn reserve(value:&mut String, capacity:i64, byte_limit:i64)->Error!void`
- [`L107`](../../lib/std/string.zil#L107) `pub fn push_scalar(value:&mut String, scalar:i32, byte_limit:i64)->Error!void`
- [`L114`](../../lib/std/string.zil#L114) `pub fn truncate(value:&mut String, scalar_count:i64)->Error!void`
- [`L120`](../../lib/std/string.zil#L120) `pub fn substring(value:&String, begin:i64, end:i64, byte_limit:i64)->Error!String`
- [`L184`](../../lib/std/string.zil#L184) `pub fn starts_with(value:&String, prefix:&[u8])->Error!bool`
- [`L190`](../../lib/std/string.zil#L190) `pub fn ends_with(value:&String, suffix:&[u8])->Error!bool`
- [`L197`](../../lib/std/string.zil#L197) `pub fn contains(value:&String, needle:&[u8])->Error!bool`
- [`L203`](../../lib/std/string.zil#L203) `pub fn trim_start(value:&String, byte_limit:i64)->Error!String`
- [`L210`](../../lib/std/string.zil#L210) `pub fn trim_end(value:&String, byte_limit:i64)->Error!String`
- [`L217`](../../lib/std/string.zil#L217) `pub fn trim(value:&String, byte_limit:i64)->Error!String`
- [`L229`](../../lib/std/string.zil#L229) `pub fn split(value:&String, sep:&[u8], byte_limit:i64)->Error!Store<String> contract {result(create)}`
- [`L269`](../../lib/std/string.zil#L269) `pub fn lines(value:&String, byte_limit:i64)->Error!Store<String> contract {result(create)}`
- [`L296`](../../lib/std/string.zil#L296) `pub fn replace_all(value:&String, from:&[u8], to:&[u8], byte_limit:i64)->Error!String`

<a id="std-int"></a>
### `std.int`

十种整数的十进制解析与格式化。源码：[`lib/std/int.zil`](../../lib/std/int.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L2`](../../lib/std/int.zil#L2) `pub enum ParseError { Empty, InvalidDigit, Overflow }`
- [`L3`](../../lib/std/int.zil#L3) `pub struct Decimal { pub bytes:[u8;20]; pub len:i64 }`

**公开函数**

- [`L5`](../../lib/std/int.zil#L5) `pub fn parse_i64(source:&[u8])->ParseError!i64`
- [`L29`](../../lib/std/int.zil#L29) `pub fn format_i64(value:i64)->Decimal`
- [`L58`](../../lib/std/int.zil#L58) `pub fn parse_u64(source:&[u8])->ParseError!u64`
- [`L75`](../../lib/std/int.zil#L75) `pub fn format_u64(value:u64)->Decimal`
- [`L92`](../../lib/std/int.zil#L92) `pub fn parse_u8(source:&[u8])->ParseError!u8`
- [`L97`](../../lib/std/int.zil#L97) `pub fn parse_u16(source:&[u8])->ParseError!u16`
- [`L102`](../../lib/std/int.zil#L102) `pub fn parse_usize(source:&[u8])->ParseError!usize`
- [`L107`](../../lib/std/int.zil#L107) `pub fn format_u8(value:u8)->Decimal`
- [`L108`](../../lib/std/int.zil#L108) `pub fn format_u16(value:u16)->Decimal`
- [`L109`](../../lib/std/int.zil#L109) `pub fn format_usize(value:usize)->Decimal`
- [`L110`](../../lib/std/int.zil#L110) `pub fn parse_i32(source:&[u8])->ParseError!i32`
- [`L115`](../../lib/std/int.zil#L115) `pub fn format_i32(value:i32)->Decimal`
- [`L117`](../../lib/std/int.zil#L117) `pub fn parse_u32(source:&[u8])->ParseError!u32`
- [`L122`](../../lib/std/int.zil#L122) `pub fn parse_i8(source:&[u8])->ParseError!i8`
- [`L127`](../../lib/std/int.zil#L127) `pub fn parse_i16(source:&[u8])->ParseError!i16`
- [`L132`](../../lib/std/int.zil#L132) `pub fn parse_isize(source:&[u8])->ParseError!isize`
- [`L138`](../../lib/std/int.zil#L138) `pub fn format_u32(value:u32)->Decimal`
- [`L139`](../../lib/std/int.zil#L139) `pub fn format_i8(value:i8)->Decimal`
- [`L140`](../../lib/std/int.zil#L140) `pub fn format_i16(value:i16)->Decimal`
- [`L141`](../../lib/std/int.zil#L141) `pub fn format_isize(value:isize)->Decimal`

<a id="std-float"></a>
### `std.float`

IEEE 754 `f32`/`f64` 文本解析与格式化。源码：[`lib/std/float.zil`](../../lib/std/float.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L14`](../../lib/std/float.zil#L14) `pub enum Error { Empty, Invalid, Range, Allocation, Locale, Format }`
- [`L15`](../../lib/std/float.zil#L15) `pub struct Decimal { pub bytes:[u8;64];pub len:i64 }`

**公开函数**

- [`L80`](../../lib/std/float.zil#L80) `pub fn parse_f64(source:&[u8])->Error!f64`
- [`L96`](../../lib/std/float.zil#L96) `pub fn parse_f32(source:&[u8])->Error!f32`
- [`L114`](../../lib/std/float.zil#L114) `pub fn format_f64(value:f64)->Error!Decimal`
- [`L124`](../../lib/std/float.zil#L124) `pub fn format_f32(value:f32)->Error!Decimal`

**Trait 实现**

- [`L17`](../../lib/std/float.zil#L17) `impl Drop for Errno`
- [`L24`](../../lib/std/float.zil#L24) `impl Drop for Locale`

<a id="std-text_edit"></a>
### `std.text_edit`

纯文本缓冲与编辑操作原语。源码：[`lib/std/text_edit.zil`](../../lib/std/text_edit.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L6`](../../lib/std/text_edit.zil#L6) `pub package Edit { manages value:ByteBuffer; pub code:i32; pub source_error:i64; pub insert_error:i64 }`
- [`L49`](../../lib/std/text_edit.zil#L49) `pub struct Status { pub code:i32; pub source_error:i64; pub insert_error:i64 }`

**公开函数**

- [`L12`](../../lib/std/text_edit.zil#L12) `pub fn replace(source:&[u8], begin:i64, end:i64, insertion:&[u8], byte_limit:i64)->Edit`
- [`L43`](../../lib/std/text_edit.zil#L43) `pub fn take(answer:Edit)->ByteBuffer contract { result(give) }`
- [`L69`](../../lib/std/text_edit.zil#L69) `pub fn replace_in_place(value:ByteBuffer, begin:i64, end:i64, insertion:&[u8], byte_limit:i64)->Status`

## 集合、存储与图（Collections, Storage & Graphs）

<a id="std-store"></a>
### `std.store`

线性拥有动态存储 `Store<T>` 封装。源码：[`lib/std/store.zil`](../../lib/std/store.zil)。

**公开函数**

- [`L2`](../../lib/std/store.zil#L2) `pub fn new<T:StoreElement>()->Store<T> contract {result(create)}`
- [`L3`](../../lib/std/store.zil#L3) `pub fn reserve<T:StoreElement>(items:Store<T>,capacity:i64)->bool`
- [`L4`](../../lib/std/store.zil#L4) `pub fn push<T:StoreElement>(items:Store<T>,value:T)->T!void`
- [`L7`](../../lib/std/store.zil#L7) `pub fn push_growing<T:StoreElement>(items:Store<T>,value:T)->T!void`
- [`L18`](../../lib/std/store.zil#L18) `pub fn pop<T:StoreElement>(items:Store<T>)->void!T`
- [`L19`](../../lib/std/store.zil#L19) `pub fn len<T:StoreElement>(items:Store<T>)->i64`
- [`L20`](../../lib/std/store.zil#L20) `pub fn capacity<T:StoreElement>(items:Store<T>)->i64`
- [`L21`](../../lib/std/store.zil#L21) `pub fn clear<T:StoreElement>(items:Store<T>)->i64`
- [`L29`](../../lib/std/store.zil#L29) `pub fn remove<T:StoreElement>(items:Store<T>,index:i64)->void!T`
- [`L47`](../../lib/std/store.zil#L47) `pub fn insert<T:StoreElement>(items:Store<T>,index:i64,value:T)->T!void`
- [`L59`](../../lib/std/store.zil#L59) `pub fn truncate<T:StoreElement>(items:Store<T>,length:i64)->bool`

<a id="std-collections"></a>
### `std.collections`

通用集合与排序辅助函数。源码：[`lib/std/collections.zil`](../../lib/std/collections.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L3`](../../lib/std/collections.zil#L3) `pub enum Error { Capacity, Allocation, Internal }`
- [`L4`](../../lib/std/collections.zil#L4) `pub struct Put { pub replaced:bool; pub previous:i64 }`
- [`L5`](../../lib/std/collections.zil#L5) `pub struct Removal { pub found:bool; pub value:i64 }`
- [`L7`](../../lib/std/collections.zil#L7) `pub package I64Map { manages entries:Store<Entry> }`
- [`L8`](../../lib/std/collections.zil#L8) `pub package I64Set { manages entries:Store<i64> }`

**公开函数**

- [`L45`](../../lib/std/collections.zil#L45) `pub fn map_new()->I64Map`
- [`L46`](../../lib/std/collections.zil#L46) `pub fn map_len(value:&I64Map)->i64`
- [`L47`](../../lib/std/collections.zil#L47) `pub fn map_capacity(value:&I64Map)->i64`
- [`L48`](../../lib/std/collections.zil#L48) `pub fn map_get(value:&I64Map,key:i64)->Option<i64>`
- [`L53`](../../lib/std/collections.zil#L53) `pub fn map_insert(value:&mut I64Map,key:i64,item:i64)->Error!Put`
- [`L73`](../../lib/std/collections.zil#L73) `pub fn map_remove(value:&mut I64Map,key:i64)->Error!Removal`
- [`L89`](../../lib/std/collections.zil#L89) `pub fn set_new()->I64Set`
- [`L90`](../../lib/std/collections.zil#L90) `pub fn set_len(value:&I64Set)->i64`
- [`L91`](../../lib/std/collections.zil#L91) `pub fn set_capacity(value:&I64Set)->i64`
- [`L92`](../../lib/std/collections.zil#L92) `pub fn set_contains(value:&I64Set,key:i64)->bool`
- [`L95`](../../lib/std/collections.zil#L95) `pub fn set_insert(value:&mut I64Set,key:i64)->Error!bool`
- [`L110`](../../lib/std/collections.zil#L110) `pub fn set_remove(value:&mut I64Set,key:i64)->Error!bool`

<a id="std-pod_hash_map"></a>
### `std.pod_hash_map`

可增长 POD 键值哈希表。源码：[`lib/std/pod_hash_map.zil`](../../lib/std/pod_hash_map.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L6`](../../lib/std/pod_hash_map.zil#L6) `pub package PodHashMap<K:Copy+StoreElement,V:Copy+StoreElement> { manages entries:Store<Entry<K,V>>; manages buckets:Store<i64>; limit:i64 }`
- [`L7`](../../lib/std/pod_hash_map.zil#L7) `pub enum Error { Capacity, Allocation, Internal }`
- [`L8`](../../lib/std/pod_hash_map.zil#L8) `pub struct Pair<K:Copy,V:Copy> { pub key:K; pub value:V }`

**公开函数**

- [`L39`](../../lib/std/pod_hash_map.zil#L39) `pub fn with_capacity<K:Copy+StoreElement,V:Copy+StoreElement>(limit:i64)->Error!PodHashMap<K,V>`
- [`L56`](../../lib/std/pod_hash_map.zil#L56) `pub fn len<K:Copy+StoreElement,V:Copy+StoreElement>(map:&PodHashMap<K,V>)->i64`
- [`L57`](../../lib/std/pod_hash_map.zil#L57) `pub fn capacity<K:Copy+StoreElement,V:Copy+StoreElement>(map:&PodHashMap<K,V>)->i64`
- [`L59`](../../lib/std/pod_hash_map.zil#L59) `pub fn at<K:Copy+StoreElement,V:Copy+StoreElement>(map:&PodHashMap<K,V>,index:i64)->Option<Pair<K,V>>`
- [`L64`](../../lib/std/pod_hash_map.zil#L64) `pub fn get<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&PodHashMap<K,V>,key:K,equals:&E,hash:&H)->Option<V>`
- [`L70`](../../lib/std/pod_hash_map.zil#L70) `pub fn insert<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&mut PodHashMap<K,V>,key:K,value:V,equals:&E,hash:&H)->Error!Option<V>`
- [`L86`](../../lib/std/pod_hash_map.zil#L86) `pub fn remove<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&mut PodHashMap<K,V>,key:K,equals:&E,hash:&H)->Error!Option<V>`
- [`L114`](../../lib/std/pod_hash_map.zil#L114) `pub fn reserve<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&mut PodHashMap<K,V>,capacity:i64,equals:&E,hash:&H)->Error!void`
- [`L136`](../../lib/std/pod_hash_map.zil#L136) `pub fn insert_growing<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&mut PodHashMap<K,V>,key:K,value:V,equals:&E,hash:&H)->Error!Option<V>`
- [`L153`](../../lib/std/pod_hash_map.zil#L153) `pub fn clear<K:Copy+StoreElement,V:Copy+StoreElement>(map:&mut PodHashMap<K,V>)->i64`

<a id="std-pod_hash_set"></a>
### `std.pod_hash_set`

可增长 POD 哈希集合。源码：[`lib/std/pod_hash_set.zil`](../../lib/std/pod_hash_set.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L5`](../../lib/std/pod_hash_set.zil#L5) `pub package PodHashSet<K:Copy+StoreElement> { state:pod_hash_map.PodHashMap<K,bool> }`

**公开函数**

- [`L7`](../../lib/std/pod_hash_set.zil#L7) `pub fn with_capacity<K:Copy+StoreElement>(capacity:i64)->pod_hash_map.Error!PodHashSet<K>`
- [`L11`](../../lib/std/pod_hash_set.zil#L11) `pub fn len<K:Copy+StoreElement>(set:&PodHashSet<K>)->i64`
- [`L12`](../../lib/std/pod_hash_set.zil#L12) `pub fn capacity<K:Copy+StoreElement>(set:&PodHashSet<K>)->i64`
- [`L13`](../../lib/std/pod_hash_set.zil#L13) `pub fn key_at<K:Copy+StoreElement>(set:&PodHashSet<K>,index:i64)->Option<K>`
- [`L19`](../../lib/std/pod_hash_set.zil#L19) `pub fn contains<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&PodHashSet<K>,key:K,equals:&E,hash:&H)->bool`
- [`L25`](../../lib/std/pod_hash_set.zil#L25) `pub fn insert<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,key:K,equals:&E,hash:&H)->pod_hash_map.Error!bool`
- [`L32`](../../lib/std/pod_hash_set.zil#L32) `pub fn insert_growing<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,key:K,equals:&E,hash:&H)->pod_hash_map.Error!bool`
- [`L39`](../../lib/std/pod_hash_set.zil#L39) `pub fn reserve<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,capacity:i64,equals:&E,hash:&H)->pod_hash_map.Error!void`
- [`L42`](../../lib/std/pod_hash_set.zil#L42) `pub fn remove<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,key:K,equals:&E,hash:&H)->pod_hash_map.Error!bool`
- [`L49`](../../lib/std/pod_hash_set.zil#L49) `pub fn clear<K:Copy+StoreElement>(set:&mut PodHashSet<K>)->i64`

<a id="std-i64_hash_map"></a>
### `std.i64_hash_map`

`i64` 键专用哈希表。源码：[`lib/std/i64_hash_map.zil`](../../lib/std/i64_hash_map.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L3`](../../lib/std/i64_hash_map.zil#L3) `pub package I64HashMap { manages entries:Store<Entry>; manages buckets:Store<i64>; limit:i64 }`
- [`L4`](../../lib/std/i64_hash_map.zil#L4) `pub enum Error { Capacity, Allocation, Internal }`
- [`L5`](../../lib/std/i64_hash_map.zil#L5) `pub struct Pair { pub key:i64; pub value:i64 }`

**公开函数**

- [`L36`](../../lib/std/i64_hash_map.zil#L36) `pub fn with_capacity(limit:i64)->Error!I64HashMap`
- [`L53`](../../lib/std/i64_hash_map.zil#L53) `pub fn len(map:&I64HashMap)->i64`
- [`L55`](../../lib/std/i64_hash_map.zil#L55) `pub fn at(map:&I64HashMap,index:i64)->Option<Pair>`
- [`L60`](../../lib/std/i64_hash_map.zil#L60) `pub fn get(map:&I64HashMap,key:i64)->Option<i64>`
- [`L66`](../../lib/std/i64_hash_map.zil#L66) `pub fn insert(map:&mut I64HashMap,key:i64,value:i64)->Error!Option<i64>`
- [`L82`](../../lib/std/i64_hash_map.zil#L82) `pub fn remove(map:&mut I64HashMap,key:i64)->Error!Option<i64>`

<a id="std-i64_hash_set"></a>
### `std.i64_hash_set`

`i64` 键专用哈希集合。源码：[`lib/std/i64_hash_set.zil`](../../lib/std/i64_hash_set.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L4`](../../lib/std/i64_hash_set.zil#L4) `pub package I64HashSet { state:i64_hash_map.I64HashMap }`

**公开函数**

- [`L5`](../../lib/std/i64_hash_set.zil#L5) `pub fn with_capacity(limit:i64)->i64_hash_map.Error!I64HashSet`
- [`L9`](../../lib/std/i64_hash_set.zil#L9) `pub fn len(set:&I64HashSet)->i64`
- [`L10`](../../lib/std/i64_hash_set.zil#L10) `pub fn key_at(set:&I64HashSet,index:i64)->Option<i64>`
- [`L16`](../../lib/std/i64_hash_set.zil#L16) `pub fn contains(set:&I64HashSet,key:i64)->bool`
- [`L22`](../../lib/std/i64_hash_set.zil#L22) `pub fn insert(set:&mut I64HashSet,key:i64)->i64_hash_map.Error!bool`
- [`L34`](../../lib/std/i64_hash_set.zil#L34) `pub fn remove(set:&mut I64HashSet,key:i64)->i64_hash_map.Error!bool`

<a id="std-string_map"></a>
### `std.string_map`

字符串键哈希映射。源码：[`lib/std/string_map.zil`](../../lib/std/string_map.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L5`](../../lib/std/string_map.zil#L5) `pub enum Failure { Capacity, Allocation, Internal }`
- [`L6`](../../lib/std/string_map.zil#L6) `pub package InsertError { pub key:string.String; pub reason:Failure }`
- [`L7`](../../lib/std/string_map.zil#L7) `pub struct Put { pub replaced:bool; pub previous:i64 }`
- [`L8`](../../lib/std/string_map.zil#L8) `pub package StringMap { manages keys:Store<string.String>; manages values:Store<i64> }`

**公开函数**

- [`L27`](../../lib/std/string_map.zil#L27) `pub fn new()->StringMap`
- [`L28`](../../lib/std/string_map.zil#L28) `pub fn len(map:&StringMap)->i64`
- [`L29`](../../lib/std/string_map.zil#L29) `pub fn get(map:&StringMap,key:&string.String)->Option<i64>`
- [`L37`](../../lib/std/string_map.zil#L37) `pub fn key_clone_at(map:&StringMap,index:i64,byte_limit:i64)->string.Error!string.String`
- [`L42`](../../lib/std/string_map.zil#L42) `pub fn value_at(map:&StringMap,index:i64)->Option<i64>`
- [`L48`](../../lib/std/string_map.zil#L48) `pub fn insert(map:&mut StringMap,key:string.String,value:i64)->InsertError!Put`
- [`L81`](../../lib/std/string_map.zil#L81) `pub fn remove(map:&mut StringMap,key:&string.String)->Failure!Option<i64>`

<a id="std-owning_entries"></a>
### `std.owning_entries`

拥有型容器的条目抽象与转移载体。源码：[`lib/std/owning_entries.zil`](../../lib/std/owning_entries.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L5`](../../lib/std/owning_entries.zil#L5) `pub package Entry<K:StoreElement,V:StoreElement> { pub key:K; pub value:V }`
- [`L6`](../../lib/std/owning_entries.zil#L6) `pub package OwningEntries<K:StoreElement,V:StoreElement> { manages items:Store<Entry<K,V>> }`

**公开函数**

- [`L8`](../../lib/std/owning_entries.zil#L8) `pub fn new<K:StoreElement,V:StoreElement>()->OwningEntries<K,V>`
- [`L12`](../../lib/std/owning_entries.zil#L12) `pub fn len<K:StoreElement,V:StoreElement>(entries:&OwningEntries<K,V>)->i64`
- [`L16`](../../lib/std/owning_entries.zil#L16) `pub fn reserve<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,capacity:i64)->bool`
- [`L21`](../../lib/std/owning_entries.zil#L21) `pub fn push<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,entry:Entry<K,V>)->Entry<K,V>!void`
- [`L27`](../../lib/std/owning_entries.zil#L27) `pub fn insert_at<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,index:i64,entry:Entry<K,V>)->Entry<K,V>!void`
- [`L35`](../../lib/std/owning_entries.zil#L35) `pub fn remove_at<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,index:i64)->void!Entry<K,V>`

<a id="std-owning_map"></a>
### `std.owning_map`

拥有型有序/基础映射。源码：[`lib/std/owning_map.zil`](../../lib/std/owning_map.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L7`](../../lib/std/owning_map.zil#L7) `pub package OwningMap<K:StoreElement+KeyEq,V:StoreElement> { manages items:Store<owning_entries.Entry<K,V>> }`

**公开 Trait**

- [`L6`](../../lib/std/owning_map.zil#L6) `pub trait KeyEq`
  - `fn same(self:&Self,other:&Self)->bool`

**公开函数**

- [`L11`](../../lib/std/owning_map.zil#L11) `pub fn new<K:StoreElement+KeyEq,V:StoreElement>()->OwningMap<K,V>`
- [`L15`](../../lib/std/owning_map.zil#L15) `pub fn len<K:StoreElement+KeyEq,V:StoreElement>(map:&OwningMap<K,V>)->i64`
- [`L20`](../../lib/std/owning_map.zil#L20) `pub fn find<K:StoreElement+KeyEq,V:StoreElement>(map:&OwningMap<K,V>,key:&K)->i64`
- [`L31`](../../lib/std/owning_map.zil#L31) `pub fn contains<K:StoreElement+KeyEq,V:StoreElement>(map:&OwningMap<K,V>,key:&K)->bool`
- [`L37`](../../lib/std/owning_map.zil#L37) `pub fn get_copy<K:StoreElement+KeyEq,V:Copy+StoreElement>(map:&OwningMap<K,V>,key:&K)->Option<V>`
- [`L45`](../../lib/std/owning_map.zil#L45) `pub fn get<life s,K:StoreElement+KeyEq,V:StoreElement>(map:&'s OwningMap<K,V>,key:&K)->Option<&'s V>`
- [`L56`](../../lib/std/owning_map.zil#L56) `pub fn insert_or_replace<K:StoreElement+KeyEq,V:StoreElement>(map:&mut OwningMap<K,V>,entry:owning_entries.Entry<K,V>)->owning_entries.Entry<K,V>!bool`
- [`L76`](../../lib/std/owning_map.zil#L76) `pub fn replace_existing<K:StoreElement+KeyEq,V:StoreElement>(map:&mut OwningMap<K,V>,entry:owning_entries.Entry<K,V>,replaced:&mut bool)->owning_entries.Entry<K,V>`
- [`L92`](../../lib/std/owning_map.zil#L92) `pub fn take<K:StoreElement+KeyEq,V:StoreElement>(map:&mut OwningMap<K,V>,key:&K)->void!owning_entries.Entry<K,V>`

<a id="std-owning_hash_map"></a>
### `std.owning_hash_map`

拥有型键值哈希表（支持 OOM 返还所有权）。源码：[`lib/std/owning_hash_map.zil`](../../lib/std/owning_hash_map.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L8`](../../lib/std/owning_hash_map.zil#L8) `pub enum Error { Capacity, Allocation, Internal }`
- [`L10`](../../lib/std/owning_hash_map.zil#L10) `pub package OwningHashMap<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement> { manages entries:Store<owning_entries.Entry<K,V>> manages buckets:Store<i64> limit:i64 }`

**公开 Trait**

- [`L7`](../../lib/std/owning_hash_map.zil#L7) `pub trait KeyHash`
  - `fn hash(self:&Self)->i64`

**公开函数**

- [`L16`](../../lib/std/owning_hash_map.zil#L16) `pub fn new<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>()->OwningHashMap<K,V>`
- [`L19`](../../lib/std/owning_hash_map.zil#L19) `pub fn len<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&OwningHashMap<K,V>)->i64`
- [`L20`](../../lib/std/owning_hash_map.zil#L20) `pub fn capacity<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&OwningHashMap<K,V>)->i64`
- [`L46`](../../lib/std/owning_hash_map.zil#L46) `pub fn contains<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&OwningHashMap<K,V>,key:&K)->bool`
- [`L47`](../../lib/std/owning_hash_map.zil#L47) `pub fn get<life s,K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&'s OwningHashMap<K,V>,key:&K)->Option<&'s V>`
- [`L53`](../../lib/std/owning_hash_map.zil#L53) `pub fn key_at<life s,K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&'s OwningHashMap<K,V>,index:i64)->Option<&'s K>`
- [`L60`](../../lib/std/owning_hash_map.zil#L60) `pub fn reserve<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>,capacity:i64)->Error!void`
- [`L98`](../../lib/std/owning_hash_map.zil#L98) `pub fn take<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>,key:&K)->void!owning_entries.Entry<K,V>`
- [`L107`](../../lib/std/owning_hash_map.zil#L107) `pub fn insert<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>,entry:owning_entries.Entry<K,V>)->owning_entries.Entry<K,V>!bool`
- [`L137`](../../lib/std/owning_hash_map.zil#L137) `pub fn clear<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>)->i64`
- [`L147`](../../lib/std/owning_hash_map.zil#L147) `pub fn into_entries<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:OwningHashMap<K,V>)->Store<owning_entries.Entry<K,V>> contract {result(give)}`

<a id="std-owning_hash_set"></a>
### `std.owning_hash_set`

拥有型哈希集合。源码：[`lib/std/owning_hash_set.zil`](../../lib/std/owning_hash_set.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L4`](../../lib/std/owning_hash_set.zil#L4) `pub package OwningHashSet<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash> { state:owning_hash_map.OwningHashMap<K,bool> }`

**公开函数**

- [`L5`](../../lib/std/owning_hash_set.zil#L5) `pub fn new<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>()->OwningHashSet<K>`
- [`L8`](../../lib/std/owning_hash_set.zil#L8) `pub fn len<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&OwningHashSet<K>)->i64`
- [`L9`](../../lib/std/owning_hash_set.zil#L9) `pub fn capacity<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&OwningHashSet<K>)->i64`
- [`L10`](../../lib/std/owning_hash_set.zil#L10) `pub fn reserve<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>,capacity:i64)->owning_hash_map.Error!void`
- [`L11`](../../lib/std/owning_hash_set.zil#L11) `pub fn contains<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&OwningHashSet<K>,key:&K)->bool`
- [`L12`](../../lib/std/owning_hash_set.zil#L12) `pub fn key_at<life s,K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&'s OwningHashSet<K>,index:i64)->Option<&'s K>`
- [`L15`](../../lib/std/owning_hash_set.zil#L15) `pub fn insert<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>,key:K)->K!bool`
- [`L22`](../../lib/std/owning_hash_set.zil#L22) `pub fn take<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>,key:&K)->void!K`
- [`L26`](../../lib/std/owning_hash_set.zil#L26) `pub fn clear<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>)->i64`
- [`L30`](../../lib/std/owning_hash_set.zil#L30) `pub fn into_entries<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:OwningHashSet<K>)->Store<owning_entries.Entry<K,bool>> contract {result(give)}`

<a id="std-graph"></a>
### `std.graph`

运行时尺寸拥有资源图（逻辑删除、回收与 ticket 复用）。源码：[`lib/std/graph.zil`](../../lib/std/graph.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L11`](../../lib/std/graph.zil#L11) `pub enum Error { Capacity, Allocation, Invalid }`
- [`L12`](../../lib/std/graph.zil#L12) `pub enum AddFailure { Capacity, Allocation }`
- [`L13`](../../lib/std/graph.zil#L13) `pub package AddError { manages payload:ByteBuffer; pub reason:AddFailure }`
- [`L14`](../../lib/std/graph.zil#L14) `pub struct NodeKey { pub ticket:i64 }`
- [`L15`](../../lib/std/graph.zil#L15) `pub struct NodeSnapshot { pub key:NodeKey; pub value:i64 }`
- [`L16`](../../lib/std/graph.zil#L16) `pub struct EdgeSnapshot { pub from:NodeKey; pub to:NodeKey }`
- [`L20`](../../lib/std/graph.zil#L20) `pub package Graph { manages nodes:Store<Node>; manages edges:Store<Edge>; manages locations:Store<i64>; next_ticket:i64; issued:i64 }`

**公开函数**

- [`L22`](../../lib/std/graph.zil#L22) `pub fn with_capacity(node_capacity:i64,edge_capacity:i64,ticket_capacity:i64)->Error!Graph`
- [`L46`](../../lib/std/graph.zil#L46) `pub fn contains(graph:&Graph,key:NodeKey)->bool`
- [`L47`](../../lib/std/graph.zil#L47) `pub fn get(graph:&Graph,key:NodeKey)->Option<NodeSnapshot>`
- [`L53`](../../lib/std/graph.zil#L53) `pub fn physical_slot(graph:&Graph,key:NodeKey)->Option<i64>`
- [`L58`](../../lib/std/graph.zil#L58) `pub fn node_count(graph:&Graph)->i64`
- [`L67`](../../lib/std/graph.zil#L67) `pub fn issued_count(graph:&Graph)->i64`
- [`L69`](../../lib/std/graph.zil#L69) `pub fn ticket_slots(graph:&Graph)->i64`
- [`L71`](../../lib/std/graph.zil#L71) `pub fn add_with_payload(graph:&mut Graph,value:i64,give payload:ByteBuffer)->AddError!NodeKey`
- [`L98`](../../lib/std/graph.zil#L98) `pub fn add(graph:&mut Graph,value:i64)->Error!NodeKey`
- [`L110`](../../lib/std/graph.zil#L110) `pub fn recover_payload(failed:AddError)->ByteBuffer contract {result(give)}`
- [`L113`](../../lib/std/graph.zil#L113) `pub fn payload_len(graph:&Graph,key:NodeKey)->Option<i64>`
- [`L119`](../../lib/std/graph.zil#L119) `pub fn payload_byte(graph:&Graph,key:NodeKey,index:i64)->Option<u8>`
- [`L129`](../../lib/std/graph.zil#L129) `pub fn remove(graph:&mut Graph,key:NodeKey)->bool`
- [`L140`](../../lib/std/graph.zil#L140) `pub fn collect(graph:&mut Graph)->Error!i64`
- [`L198`](../../lib/std/graph.zil#L198) `pub fn link(graph:&mut Graph,from:NodeKey,to:NodeKey)->Error!i64`
- [`L208`](../../lib/std/graph.zil#L208) `pub fn edge(graph:&Graph,index:i64)->Option<EdgeSnapshot>`
- [`L216`](../../lib/std/graph.zil#L216) `pub fn edge_count(graph:&Graph)->i64`
- [`L231`](../../lib/std/graph.zil#L231) `pub fn take_payload(graph:&mut Graph,key:NodeKey)->Error!ByteBuffer contract {result(give)}`
- [`L252`](../../lib/std/graph.zil#L252) `pub fn clear(graph:&mut Graph)->i64`

## 迭代与算法（Iteration）

<a id="std-iter"></a>
### `std.iter`

Copy 迭代、`LendingIterator` 与拥有型 `Iterator`/`Map`/`Filter`/`Drain`。源码：[`lib/std/iter.zil`](../../lib/std/iter.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L108`](../../lib/std/iter.zil#L108) `pub package Drain<T:StoreElement+SyncDrop> { manages items:Store<T> }`
- [`L131`](../../lib/std/iter.zil#L131) `pub package Map<I:Iterator+Value+SyncDrop,M:Mapper<I.Item>+Value+SyncDrop> { input:I; callback:M }`
- [`L143`](../../lib/std/iter.zil#L143) `pub package Filter<I:Iterator+Value+SyncDrop,P:Predicate<I.Item>+Value+SyncDrop> { input:I; predicate:P }`
- [`L159`](../../lib/std/iter.zil#L159) `pub package Collected<T:StoreElement+SyncDrop> { manages items:Store<T> }`

**公开 Trait**

- [`L77`](../../lib/std/iter.zil#L77) `pub trait LendingIterator`
  - `type Item`
  - `fn next<life a>(self:&'a mut Self)->Option<&'a Self.Item>`
- [`L93`](../../lib/std/iter.zil#L93) `pub trait Iterator`
  - `type Item:Value+SyncDrop`
  - `fn next(self:&mut Self)->void!Self.Item`
- [`L99`](../../lib/std/iter.zil#L99) `pub trait Mapper<T:Value+SyncDrop>`
  - `type Output:Value+SyncDrop`
  - `fn map(self:&mut Self,value:T)->Self.Output`
- [`L103`](../../lib/std/iter.zil#L103) `pub trait Predicate<T:Value+SyncDrop>`
  - `fn keep(self:&mut Self,value:&T)->bool`

**公开函数**

- [`L4`](../../lib/std/iter.zil#L4) `pub fn for_each<T:Copy,F:FnMut(T)->void>(values:&[T],callback:&mut F)`
- [`L8`](../../lib/std/iter.zil#L8) `pub fn fold<T:Copy,F:FnMut(T,T)->T>(values:&[T],initial:T,callback:&mut F)->T`
- [`L15`](../../lib/std/iter.zil#L15) `pub fn any<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->bool`
- [`L20`](../../lib/std/iter.zil#L20) `pub fn all<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->bool`
- [`L25`](../../lib/std/iter.zil#L25) `pub fn position<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->i64?`
- [`L30`](../../lib/std/iter.zil#L30) `pub fn find<T:Copy,F:FnMut(T)->bool>(values:&[T],predicate:&mut F)->T?`
- [`L38`](../../lib/std/iter.zil#L38) `pub fn sort_unstable_by<T:Copy,F:FnMut(T,T)->bool>(values:&mut [T],less:&mut F)`
- [`L84`](../../lib/std/iter.zil#L84) `pub fn next_copy<I:LendingIterator>(cursor:&mut I)->Option<I.Item> where I.Item:Copy`
- [`L109`](../../lib/std/iter.zil#L109) `pub fn drain<T:StoreElement+SyncDrop>(give items:Store<T>)->Drain<T>`
- [`L119`](../../lib/std/iter.zil#L119) `pub fn into_iter<T:StoreElement+SyncDrop>(give items:Store<T>)->void!Drain<T>`
- [`L132`](../../lib/std/iter.zil#L132) `pub fn map<I:Iterator+Value+SyncDrop,M:Mapper<I.Item>+Value+SyncDrop>(input:I,callback:M)->Map<I,M>`
- [`L144`](../../lib/std/iter.zil#L144) `pub fn filter<I:Iterator+Value+SyncDrop,P:Predicate<I.Item>+Value+SyncDrop>(input:I,predicate:P)->Filter<I,P>`
- [`L157`](../../lib/std/iter.zil#L157) `pub fn next<I:Iterator>(cursor:&mut I)->void!I.Item`
- [`L162`](../../lib/std/iter.zil#L162) `pub fn collect<I:Iterator+Value+SyncDrop>(input:I)->void!Collected<I.Item> where I.Item:StoreElement`
- [`L178`](../../lib/std/iter.zil#L178) `pub fn for_each_owned<I:Iterator+Value,F:FnMut(I.Item)->void>(cursor:&mut I,callback:&mut F)`
- [`L187`](../../lib/std/iter.zil#L187) `pub fn for_each_lent<I:LendingIterator+Value,F:for<life a> FnMut(&'a I.Item)->void>(cursor:&mut I,callback:&mut F)`
- [`L197`](../../lib/std/iter.zil#L197) `pub fn into_store<T:StoreElement+SyncDrop>(values:Collected<T>)->Store<T> contract {result(give)}`
- [`L201`](../../lib/std/iter.zil#L201) `pub fn fold_owned<I:Iterator+Value+SyncDrop,A:Value+SyncDrop,F:FnMut(A,I.Item)->A>(input:I,initial:A,callback:&mut F)->A`

**Trait 实现**

- [`L112`](../../lib/std/iter.zil#L112) `impl<T:StoreElement+SyncDrop> Iterator for Drain<T>`
- [`L135`](../../lib/std/iter.zil#L135) `impl<I:Iterator+Value+SyncDrop,M:Mapper<I.Item>+Value+SyncDrop> Iterator for Map<I,M>`
- [`L147`](../../lib/std/iter.zil#L147) `impl<I:Iterator+Value+SyncDrop,P:Predicate<I.Item>+Value+SyncDrop> Iterator for Filter<I,P>`

## I/O、文件系统、网络与环境（I/O, Filesystem, Network & System）

<a id="std-io"></a>
### `std.io`

基础文件句柄与标准 I/O 错误类型。源码：[`lib/std/io.zil`](../../lib/std/io.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L15`](../../lib/std/io.zil#L15) `pub enum Error { Duplicate, Open, InvalidPath, Capacity, Allocation, Read, Write, NoProgress, InvalidResult }`
- [`L16`](../../lib/std/io.zil#L16) `pub struct ReadResult { pub count:i64; pub eof:bool }`
- [`L17`](../../lib/std/io.zil#L17) `pub resource File { fd:i32 }`

**公开函数**

- [`L22`](../../lib/std/io.zil#L22) `pub fn duplicate(fd:i32)->Error!File contract {result(create)}`
- [`L28`](../../lib/std/io.zil#L28) `pub fn stdin()->Error!File contract {result(create)}`
- [`L29`](../../lib/std/io.zil#L29) `pub fn stdout()->Error!File contract {result(create)}`
- [`L30`](../../lib/std/io.zil#L30) `pub fn stderr()->Error!File contract {result(create)}`
- [`L53`](../../lib/std/io.zil#L53) `pub fn open_read(path:&[u8])->Error!File contract {result(create)}`
- [`L60`](../../lib/std/io.zil#L60) `pub fn create_write(path:&[u8],mode:i32)->Error!File contract {result(create)}`
- [`L68`](../../lib/std/io.zil#L68) `pub fn empty_buffer()->[u8;64]`
- [`L82`](../../lib/std/io.zil#L82) `pub fn read_chunk(file:&File,buffer:&mut [u8;64])->Error!ReadResult`
- [`L104`](../../lib/std/io.zil#L104) `pub fn read_to_end(file:&File,byte_limit:i64)->Error!ByteBuffer contract {result(create)}`
- [`L168`](../../lib/std/io.zil#L168) `pub fn write_all(file:&File,source:&[u8])->Error!void`
- [`L172`](../../lib/std/io.zil#L172) `pub fn print(bytes:&[u8])->Error!void`
- [`L176`](../../lib/std/io.zil#L176) `pub fn println(bytes:&[u8])->Error!void`
- [`L181`](../../lib/std/io.zil#L181) `pub fn eprint(bytes:&[u8])->Error!void`
- [`L185`](../../lib/std/io.zil#L185) `pub fn eprintln(bytes:&[u8])->Error!void`
- [`L190`](../../lib/std/io.zil#L190) `pub fn print_i64(value:i64)->Error!void`
- [`L211`](../../lib/std/io.zil#L211) `pub fn println_i64(value:i64)->Error!void`
- [`L219`](../../lib/std/io.zil#L219) `pub fn write_chunk(file:&File,buffer:&[u8;64],count:i64)->Error!i64`

**Trait 实现**

- [`L18`](../../lib/std/io.zil#L18) `impl Drop for File`

<a id="std-buffered_io"></a>
### `std.buffered_io`

固定容量缓冲读写器（`BufReader` / `BufWriter`）。源码：[`lib/std/buffered_io.zil`](../../lib/std/buffered_io.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L5`](../../lib/std/buffered_io.zil#L5) `pub package Reader { manages file:io.File; block:[u8;64]; begin:i64; end:i64; eof:bool }`
- [`L6`](../../lib/std/buffered_io.zil#L6) `pub package Writer { manages file:io.File; block:[u8;64]; used:i64 }`
- [`L66`](../../lib/std/buffered_io.zil#L66) `pub package Finished { manages file:io.File }`

**公开函数**

- [`L7`](../../lib/std/buffered_io.zil#L7) `pub fn reader(give file:io.File)->Reader`
- [`L8`](../../lib/std/buffered_io.zil#L8) `pub fn writer(give file:io.File)->Writer`
- [`L9`](../../lib/std/buffered_io.zil#L9) `pub fn pending(value:&Writer)->i64`
- [`L10`](../../lib/std/buffered_io.zil#L10) `pub fn buffered(value:&Reader)->i64`
- [`L11`](../../lib/std/buffered_io.zil#L11) `pub fn read_byte(value:&mut Reader)->io.Error!Option<u8>`
- [`L26`](../../lib/std/buffered_io.zil#L26) `pub fn read(value:&mut Reader,output:&mut [u8])->io.Error!i64`
- [`L37`](../../lib/std/buffered_io.zil#L37) `pub fn flush(value:&mut Writer)->io.Error!void`
- [`L46`](../../lib/std/buffered_io.zil#L46) `pub fn write_byte(value:&mut Writer,byte:u8)->io.Error!void`
- [`L55`](../../lib/std/buffered_io.zil#L55) `pub fn write(value:&mut Writer,source:&[u8])->io.Error!i64`
- [`L67`](../../lib/std/buffered_io.zil#L67) `pub fn into_file(value:Finished)->io.File contract {result(give)}`
- [`L68`](../../lib/std/buffered_io.zil#L68) `pub fn finish(value:Writer)->Writer!Finished`

<a id="std-fs"></a>
### `std.fs`

文件读写、目录遍历、元数据与重命名。源码：[`lib/std/fs.zil`](../../lib/std/fs.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L25`](../../lib/std/fs.zil#L25) `pub enum RemoveError { InvalidPath, Allocation, InvalidResult, Remove }`
- [`L26`](../../lib/std/fs.zil#L26) `pub struct Metadata { pub size:i64; pub is_dir:bool; pub is_file:bool }`

**公开函数**

- [`L110`](../../lib/std/fs.zil#L110) `pub fn read_to_bytes(path:&[u8])->io.Error!ByteBuffer contract {result(create)}`
- [`L116`](../../lib/std/fs.zil#L116) `pub fn write_bytes(path:&[u8],data:&[u8])->io.Error!void`
- [`L122`](../../lib/std/fs.zil#L122) `pub fn exists(path:&[u8])->bool`
- [`L134`](../../lib/std/fs.zil#L134) `pub fn remove_file(path:&[u8])->RemoveError!void`
- [`L144`](../../lib/std/fs.zil#L144) `pub fn metadata(path:&[u8])->io.Error!Metadata`
- [`L161`](../../lib/std/fs.zil#L161) `pub fn is_dir(path:&[u8])->bool`
- [`L169`](../../lib/std/fs.zil#L169) `pub fn is_file(path:&[u8])->bool`
- [`L177`](../../lib/std/fs.zil#L177) `pub fn file_size(path:&[u8])->io.Error!i64`
- [`L183`](../../lib/std/fs.zil#L183) `pub fn mkdir(path:&[u8])->io.Error!void`
- [`L194`](../../lib/std/fs.zil#L194) `pub fn mkdir_all(path:&[u8])->io.Error!void`
- [`L229`](../../lib/std/fs.zil#L229) `pub fn remove_dir(path:&[u8])->io.Error!void`
- [`L238`](../../lib/std/fs.zil#L238) `pub fn rename(from:&[u8],to:&[u8])->io.Error!void`
- [`L249`](../../lib/std/fs.zil#L249) `pub fn read_dir(path:&[u8])->io.Error!Store<Managed<ByteBuffer>> contract {result(create)}`

**Trait 实现**

- [`L29`](../../lib/std/fs.zil#L29) `impl Drop for DirStream`

<a id="std-path"></a>
### `std.path`

路径拼接、分割与扩展名操作。源码：[`lib/std/path.zil`](../../lib/std/path.zil)。

**公开函数**

- [`L24`](../../lib/std/path.zil#L24) `pub fn join(base:&[u8],child:&[u8])->io.Error!ByteBuffer contract {result(create)}`
- [`L55`](../../lib/std/path.zil#L55) `pub fn file_name(path:&[u8])->&[u8]`
- [`L65`](../../lib/std/path.zil#L65) `pub fn parent(path:&[u8])->&[u8]`
- [`L79`](../../lib/std/path.zil#L79) `pub fn extension(path:&[u8])->&[u8]`
- [`L94`](../../lib/std/path.zil#L94) `pub fn normalize(path:&[u8])->io.Error!ByteBuffer contract {result(create)}`

<a id="std-env"></a>
### `std.env`

命令行参数与进程环境访问。源码：[`lib/std/env.zil`](../../lib/std/env.zil)。

**公开函数**

- [`L73`](../../lib/std/env.zil#L73) `pub fn get(name:&[u8])->Option<Managed<ByteBuffer>>`
- [`L89`](../../lib/std/env.zil#L89) `pub fn arg_count()->i64`
- [`L98`](../../lib/std/env.zil#L98) `pub fn arg(index:i64)->Option<Managed<ByteBuffer>>`
- [`L109`](../../lib/std/env.zil#L109) `pub fn args()->io.Error!Store<Managed<ByteBuffer>> contract {result(create)}`
- [`L137`](../../lib/std/env.zil#L137) `pub fn current_dir()->io.Error!ByteBuffer contract {result(create)}`
- [`L167`](../../lib/std/env.zil#L167) `pub fn exit(code:i32)`

<a id="std-time"></a>
### `std.time`

单调时钟与墙上时间查询。源码：[`lib/std/time.zil`](../../lib/std/time.zil)。

**公开函数**

- [`L13`](../../lib/std/time.zil#L13) `pub fn monotonic_ns()->u64`
- [`L22`](../../lib/std/time.zil#L22) `pub fn sleep_ms(ms:u64)`

<a id="std-net"></a>
### `std.net`

IPv4 TCP 监听与连接（`TcpListener` / `TcpStream`）。源码：[`lib/std/net.zil`](../../lib/std/net.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L37`](../../lib/std/net.zil#L37) `pub struct Ipv4Addr { pub a: u8; pub b: u8; pub c: u8; pub d: u8 }`
- [`L102`](../../lib/std/net.zil#L102) `pub resource TcpStream { fd: i32 }`
- [`L301`](../../lib/std/net.zil#L301) `pub resource TcpListener { fd: i32 }`

**方法（`impl TcpStream`，[`L180`](../../lib/std/net.zil#L180)）**

- [`L181`](../../lib/std/net.zil#L181) `pub fn connect(addr: Ipv4Addr, port: i32) -> io.Error!TcpStream contract { result(create) }`
- [`L201`](../../lib/std/net.zil#L201) `pub fn close(self: &mut TcpStream) -> io.Error!void`
- [`L215`](../../lib/std/net.zil#L215) `pub fn read(self: &mut TcpStream, buf: &mut [u8]) -> io.Error!i64`
- [`L219`](../../lib/std/net.zil#L219) `pub fn read_to_end(self: &mut TcpStream, byte_limit: i64) -> io.Error!ByteBuffer contract { result(create) }`
- [`L269`](../../lib/std/net.zil#L269) `pub fn write(self: &mut TcpStream, data: &[u8]) -> io.Error!i64`
- [`L273`](../../lib/std/net.zil#L273) `pub fn write_all(self: &mut TcpStream, data: &[u8]) -> io.Error!void`
- [`L287`](../../lib/std/net.zil#L287) `pub fn shutdown_write(self: &mut TcpStream) -> io.Error!void`

**方法（`impl TcpListener`，[`L313`](../../lib/std/net.zil#L313)）**

- [`L314`](../../lib/std/net.zil#L314) `pub fn bind(addr: Ipv4Addr, port: i32) -> io.Error!TcpListener contract { result(create) }`
- [`L348`](../../lib/std/net.zil#L348) `pub fn local_port(self: &TcpListener) -> io.Error!i32`
- [`L362`](../../lib/std/net.zil#L362) `pub fn accept(self: &TcpListener) -> io.Error!TcpStream contract { result(create) }`
- [`L386`](../../lib/std/net.zil#L386) `pub fn close(self: &mut TcpListener) -> io.Error!void`

**公开函数**

- [`L44`](../../lib/std/net.zil#L44) `pub fn ipv4(a: u8, b: u8, c: u8, d: u8) -> Ipv4Addr`
- [`L48`](../../lib/std/net.zil#L48) `pub fn localhost() -> Ipv4Addr`

**Trait 实现**

- [`L106`](../../lib/std/net.zil#L106) `impl Drop for TcpStream`
- [`L305`](../../lib/std/net.zil#L305) `impl Drop for TcpListener`

## 界面与图形基础（UI Foundations）

<a id="std-ui_input"></a>
### `std.ui_input`

UI 输入事件基础数据结构。源码：[`lib/std/ui_input.zil`](../../lib/std/ui_input.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L6`](../../lib/std/ui_input.zil#L6) `pub enum Status { Done, Inactive, Stale }`
- [`L7`](../../lib/std/ui_input.zil#L7) `pub package Composition { text:string.String; active:bool;begin:i64;end:i64;cursor:i64;revision:u64 }`

**公开函数**

- [`L11`](../../lib/std/ui_input.zil#L11) `pub fn new()->Composition`
- [`L12`](../../lib/std/ui_input.zil#L12) `pub fn active(session:&Composition)->bool`
- [`L13`](../../lib/std/ui_input.zil#L13) `pub fn cursor(session:&Composition)->i64`
- [`L14`](../../lib/std/ui_input.zil#L14) `pub fn preedit(session:&Composition)->&[u8]`
- [`L16`](../../lib/std/ui_input.zil#L16) `pub fn begin(session:&mut Composition,document:&string.String,first:i64,last:i64,revision:u64)->string.Error!Status`
- [`L25`](../../lib/std/ui_input.zil#L25) `pub fn update(session:&mut Composition,text:&[u8],cursor:i64,byte_limit:i64)->string.Error!Status`
- [`L35`](../../lib/std/ui_input.zil#L35) `pub fn cancel(session:&mut Composition)->Status`
- [`L42`](../../lib/std/ui_input.zil#L42) `pub fn commit(session:&mut Composition,document:&mut string.String,revision:u64,text:&[u8],byte_limit:i64)->string.Error!Status`

<a id="std-ui_layout"></a>
### `std.ui_layout`

UI 布局几何计算基础。源码：[`lib/std/ui_layout.zil`](../../lib/std/ui_layout.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L3`](../../lib/std/ui_layout.zil#L3) `pub struct Rect { pub x:i64; pub y:i64; pub width:i64; pub height:i64 }`
- [`L4`](../../lib/std/ui_layout.zil#L4) `pub struct Track { pub min:i64; pub max:i64; pub flex:bool }`
- [`L5`](../../lib/std/ui_layout.zil#L5) `pub enum Error { Invalid, Space, Output }`

**公开函数**

- [`L8`](../../lib/std/ui_layout.zil#L8) `pub fn valid(rect:Rect)->bool`
- [`L11`](../../lib/std/ui_layout.zil#L11) `pub fn contains(rect:Rect,x:i64,y:i64)->bool`
- [`L14`](../../lib/std/ui_layout.zil#L14) `pub fn intersect(a:Rect,b:Rect)->Error!Rect`
- [`L76`](../../lib/std/ui_layout.zil#L76) `pub fn row(bounds:Rect,tracks:&[Track],gap:i64,output:&mut [Rect])->Error!void`
- [`L77`](../../lib/std/ui_layout.zil#L77) `pub fn column(bounds:Rect,tracks:&[Track],gap:i64,output:&mut [Rect])->Error!void`

<a id="std-ui_raster"></a>
### `std.ui_raster`

UI 软件光栅化基础原语。源码：[`lib/std/ui_raster.zil`](../../lib/std/ui_raster.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L3`](../../lib/std/ui_raster.zil#L3) `pub struct Color { pub r:u8; pub g:u8; pub b:u8; pub a:u8 }`
- [`L4`](../../lib/std/ui_raster.zil#L4) `pub enum Error { Invalid, Buffer }`

**公开函数**

- [`L21`](../../lib/std/ui_raster.zil#L21) `pub fn fill(data:&mut [u8],width:i64,height:i64,stride:i64,rect:layout.Rect,color:Color,blend:bool)->Error!void`

## Windows 平台模块（`lib/std/windows/`）

Windows 目标编译时，`std.env` / `std.float` / `std.fs` / `std.io` / `std.net` / `std.path` / `std.time` 的底层实现由下列 Win32/UCRT/Winsock2 模块提供，保持与跨平台接口契约一致。

<a id="std-windows-env"></a>
### `std.windows.env`

Windows 平台命令行与环境适配。源码：[`lib/std/windows/env.zil`](../../lib/std/windows/env.zil)。

**公开函数**

- [`L152`](../../lib/std/windows/env.zil#L152) `pub fn get(name: &[u8]) -> Option<Managed<ByteBuffer>>`
- [`L167`](../../lib/std/windows/env.zil#L167) `pub fn current_dir() -> io.Error!ByteBuffer contract { result(create) }`
- [`L299`](../../lib/std/windows/env.zil#L299) `pub fn args() -> io.Error!Store<Managed<ByteBuffer>> contract { result(create) }`
- [`L305`](../../lib/std/windows/env.zil#L305) `pub fn arg_count() -> i64`
- [`L313`](../../lib/std/windows/env.zil#L313) `pub fn arg(index: i64) -> Option<Managed<ByteBuffer>>`
- [`L335`](../../lib/std/windows/env.zil#L335) `pub fn exit(code: i32)`

<a id="std-windows-float"></a>
### `std.windows.float`

Windows 平台浮点文本解析与格式化适配。源码：[`lib/std/windows/float.zil`](../../lib/std/windows/float.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L17`](../../lib/std/windows/float.zil#L17) `pub enum Error { Empty, Invalid, Range, Allocation, Locale, Format }`
- [`L18`](../../lib/std/windows/float.zil#L18) `pub struct Decimal { pub bytes: [u8; 64]; pub len: i64 }`

**公开函数**

- [`L102`](../../lib/std/windows/float.zil#L102) `pub fn parse_f64(source: &[u8]) -> Error!f64`
- [`L125`](../../lib/std/windows/float.zil#L125) `pub fn parse_f32(source: &[u8]) -> Error!f32`
- [`L148`](../../lib/std/windows/float.zil#L148) `pub fn format_f64(value: f64) -> Error!Decimal`
- [`L159`](../../lib/std/windows/float.zil#L159) `pub fn format_f32(value: f32) -> Error!Decimal`

**Trait 实现**

- [`L21`](../../lib/std/windows/float.zil#L21) `impl Drop for Errno`
- [`L29`](../../lib/std/windows/float.zil#L29) `impl Drop for Locale`

<a id="std-windows-fs"></a>
### `std.windows.fs`

Windows Win32 Unicode (`*W`) 文件系统适配。源码：[`lib/std/windows/fs.zil`](../../lib/std/windows/fs.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L39`](../../lib/std/windows/fs.zil#L39) `pub enum RemoveError { InvalidPath, Allocation, InvalidResult, Remove }`
- [`L40`](../../lib/std/windows/fs.zil#L40) `pub struct Metadata { pub size: i64; pub is_dir: bool; pub is_file: bool }`

**公开函数**

- [`L212`](../../lib/std/windows/fs.zil#L212) `pub fn read_to_bytes(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) }`
- [`L218`](../../lib/std/windows/fs.zil#L218) `pub fn write_bytes(path: &[u8], data: &[u8]) -> io.Error!void`
- [`L224`](../../lib/std/windows/fs.zil#L224) `pub fn exists(path: &[u8]) -> bool`
- [`L237`](../../lib/std/windows/fs.zil#L237) `pub fn remove_file(path: &[u8]) -> RemoveError!void`
- [`L246`](../../lib/std/windows/fs.zil#L246) `pub fn metadata(path: &[u8]) -> io.Error!Metadata`
- [`L271`](../../lib/std/windows/fs.zil#L271) `pub fn is_dir(path: &[u8]) -> bool`
- [`L279`](../../lib/std/windows/fs.zil#L279) `pub fn is_file(path: &[u8]) -> bool`
- [`L287`](../../lib/std/windows/fs.zil#L287) `pub fn file_size(path: &[u8]) -> io.Error!i64`
- [`L293`](../../lib/std/windows/fs.zil#L293) `pub fn mkdir(path: &[u8]) -> io.Error!void`
- [`L303`](../../lib/std/windows/fs.zil#L303) `pub fn mkdir_all(path: &[u8]) -> io.Error!void`
- [`L337`](../../lib/std/windows/fs.zil#L337) `pub fn remove_dir(path: &[u8]) -> io.Error!void`
- [`L346`](../../lib/std/windows/fs.zil#L346) `pub fn rename(from: &[u8], to: &[u8]) -> io.Error!void`
- [`L357`](../../lib/std/windows/fs.zil#L357) `pub fn read_dir(path: &[u8]) -> io.Error!Store<Managed<ByteBuffer>> contract { result(create) }`

**Trait 实现**

- [`L43`](../../lib/std/windows/fs.zil#L43) `impl Drop for FindHandle`

<a id="std-windows-io"></a>
### `std.windows.io`

Windows 平台基础 I/O 与错误适配。源码：[`lib/std/windows/io.zil`](../../lib/std/windows/io.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L18`](../../lib/std/windows/io.zil#L18) `pub enum Error { Duplicate, Open, InvalidPath, Capacity, Allocation, Read, Write, NoProgress, InvalidResult }`
- [`L19`](../../lib/std/windows/io.zil#L19) `pub struct ReadResult { pub count: i64; pub eof: bool }`
- [`L20`](../../lib/std/windows/io.zil#L20) `pub resource File { fd: i32 }`

**公开函数**

- [`L25`](../../lib/std/windows/io.zil#L25) `pub fn duplicate(fd: i32) -> Error!File contract { result(create) }`
- [`L33`](../../lib/std/windows/io.zil#L33) `pub fn stdin() -> Error!File contract { result(create) }`
- [`L34`](../../lib/std/windows/io.zil#L34) `pub fn stdout() -> Error!File contract { result(create) }`
- [`L35`](../../lib/std/windows/io.zil#L35) `pub fn stderr() -> Error!File contract { result(create) }`
- [`L67`](../../lib/std/windows/io.zil#L67) `pub fn open_read(path: &[u8]) -> Error!File contract { result(create) }`
- [`L78`](../../lib/std/windows/io.zil#L78) `pub fn create_write(path: &[u8], _mode: i32) -> Error!File contract { result(create) }`
- [`L89`](../../lib/std/windows/io.zil#L89) `pub fn empty_buffer() -> [u8; 64]`
- [`L102`](../../lib/std/windows/io.zil#L102) `pub fn read_chunk(file: &File, buffer: &mut [u8; 64]) -> Error!ReadResult`
- [`L122`](../../lib/std/windows/io.zil#L122) `pub fn read_to_end(file: &File, byte_limit: i64) -> Error!ByteBuffer contract { result(create) }`
- [`L188`](../../lib/std/windows/io.zil#L188) `pub fn write_all(file: &File, source: &[u8]) -> Error!void`
- [`L192`](../../lib/std/windows/io.zil#L192) `pub fn print(bytes: &[u8]) -> Error!void`
- [`L196`](../../lib/std/windows/io.zil#L196) `pub fn println(bytes: &[u8]) -> Error!void`
- [`L201`](../../lib/std/windows/io.zil#L201) `pub fn eprint(bytes: &[u8]) -> Error!void`
- [`L205`](../../lib/std/windows/io.zil#L205) `pub fn eprintln(bytes: &[u8]) -> Error!void`
- [`L210`](../../lib/std/windows/io.zil#L210) `pub fn print_i64(value: i64) -> Error!void`
- [`L231`](../../lib/std/windows/io.zil#L231) `pub fn println_i64(value: i64) -> Error!void`
- [`L236`](../../lib/std/windows/io.zil#L236) `pub fn write_chunk(file: &File, buffer: &[u8; 64], count: i64) -> Error!i64`

**Trait 实现**

- [`L21`](../../lib/std/windows/io.zil#L21) `impl Drop for File`

<a id="std-windows-net"></a>
### `std.windows.net`

Windows Winsock2 TCP 网络适配。源码：[`lib/std/windows/net.zil`](../../lib/std/windows/net.zil)。

**公开类型（Types / Resources / Packages / Enums）**

- [`L53`](../../lib/std/windows/net.zil#L53) `pub struct Ipv4Addr { pub a: u8; pub b: u8; pub c: u8; pub d: u8 }`
- [`L145`](../../lib/std/windows/net.zil#L145) `pub resource TcpStream { sock: i64; wsa_active: bool }`
- [`L365`](../../lib/std/windows/net.zil#L365) `pub resource TcpListener { sock: i64; wsa_active: bool }`

**方法（`impl TcpStream`，[`L234`](../../lib/std/windows/net.zil#L234)）**

- [`L235`](../../lib/std/windows/net.zil#L235) `pub fn connect(addr: Ipv4Addr, port: i32) -> io.Error!TcpStream contract { result(create) }`
- [`L258`](../../lib/std/windows/net.zil#L258) `pub fn close(self: &mut TcpStream) -> io.Error!void`
- [`L279`](../../lib/std/windows/net.zil#L279) `pub fn read(self: &mut TcpStream, buf: &mut [u8]) -> io.Error!i64`
- [`L283`](../../lib/std/windows/net.zil#L283) `pub fn read_to_end(self: &mut TcpStream, byte_limit: i64) -> io.Error!ByteBuffer contract { result(create) }`
- [`L333`](../../lib/std/windows/net.zil#L333) `pub fn write(self: &mut TcpStream, data: &[u8]) -> io.Error!i64`
- [`L337`](../../lib/std/windows/net.zil#L337) `pub fn write_all(self: &mut TcpStream, data: &[u8]) -> io.Error!void`
- [`L351`](../../lib/std/windows/net.zil#L351) `pub fn shutdown_write(self: &mut TcpStream) -> io.Error!void`

**方法（`impl TcpListener`，[`L381`](../../lib/std/windows/net.zil#L381)）**

- [`L382`](../../lib/std/windows/net.zil#L382) `pub fn bind(addr: Ipv4Addr, port: i32) -> io.Error!TcpListener contract { result(create) }`
- [`L421`](../../lib/std/windows/net.zil#L421) `pub fn local_port(self: &TcpListener) -> io.Error!i32`
- [`L435`](../../lib/std/windows/net.zil#L435) `pub fn accept(self: &TcpListener) -> io.Error!TcpStream contract { result(create) }`
- [`L467`](../../lib/std/windows/net.zil#L467) `pub fn close(self: &mut TcpListener) -> io.Error!void`

**公开函数**

- [`L60`](../../lib/std/windows/net.zil#L60) `pub fn ipv4(a: u8, b: u8, c: u8, d: u8) -> Ipv4Addr`
- [`L64`](../../lib/std/windows/net.zil#L64) `pub fn localhost() -> Ipv4Addr`

**Trait 实现**

- [`L150`](../../lib/std/windows/net.zil#L150) `impl Drop for TcpStream`
- [`L370`](../../lib/std/windows/net.zil#L370) `impl Drop for TcpListener`

<a id="std-windows-path"></a>
### `std.windows.path`

Windows 路径分隔符与前缀处理。源码：[`lib/std/windows/path.zil`](../../lib/std/windows/path.zil)。

**公开函数**

- [`L50`](../../lib/std/windows/path.zil#L50) `pub fn join(base: &[u8], child: &[u8]) -> io.Error!ByteBuffer contract { result(create) }`
- [`L82`](../../lib/std/windows/path.zil#L82) `pub fn file_name(path: &[u8]) -> &[u8]`
- [`L94`](../../lib/std/windows/path.zil#L94) `pub fn parent(path: &[u8]) -> &[u8]`
- [`L115`](../../lib/std/windows/path.zil#L115) `pub fn extension(path: &[u8]) -> &[u8]`
- [`L131`](../../lib/std/windows/path.zil#L131) `pub fn normalize(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) }`

<a id="std-windows-time"></a>
### `std.windows.time`

Windows 高精度时钟适配。源码：[`lib/std/windows/time.zil`](../../lib/std/windows/time.zil)。

**公开函数**

- [`L9`](../../lib/std/windows/time.zil#L9) `pub fn monotonic_ns() -> u64`
- [`L29`](../../lib/std/windows/time.zil#L29) `pub fn sleep_ms(ms: u64)`
