---
id: kd_cb3d151e-7087-4330-ba27-85114b647e84
type: skill
path: gc.md
title: gc
inheritInjectMode: true
summaryEnabled: true
commandEnabled: true
readOnly: false
aiMaintained: false
explicitMaintenanceRules: false
maintenanceRulesCache: |-
  ## Maintenance Rules
  - 保持代码示例简洁且可执行
  - 问题模式速查表保持与项目同步
  - 添加新的项目特定优化经验到相应章节
skillEnabled: true
skillSurface: command
commandTrigger: /gc
createdAt: 1779272454862
updatedAt: 1779761898139
---

# gc

## Summary
Unity C# 和 xLua 项目中的 GC 热点分析与优化指南，包含通用模式、项目特定问题速查表、已验证优化方案和编码规范。Play Mode 下优先用 Locus **Lua GC 监控面板**录制，再调用 Agent 工具 **`lua_gc_analyze`** 获取运行时告警，与本技能的静态模式对照给出建议。

## Content

### 运行时数据（Locus）

1. Unity **Play Mode** 中打开 Locus 会话工具栏 **Lua GC**，开始录制（默认 100ms）。
2. Agent 调用 **`lua_gc_analyze`** 读取当前会话的规则告警（分配尖峰、持续 GC 债务、泄漏风险、atomic 阶段占比）与最近采样。
3. 将告警与下方静态模式对照；导出文件位于 `Library/Locus/LuaGc/<sessionId>/`。
4. 详见 `docs/lua-gc-monitor.md`。

## 二、Lua（xLua）侧常见 GC 热点

### 1. 频繁创建临时 table

- 每帧 `local t = {}` 用作临时字典或数组。
- 函数返回多个值时返回 `{x, y, z}` 表。
- **`table.pack()` 等也会产生新表**。
- **优化：复用表（清空后重用），或使用多个返回值**。

```lua
-- ❌ 每帧产生新表：传递参数、函数返回多个值
function Update()
    local pos = { x=1, y=2, z=3 }
    SetPosition(pos)          -- 表用后即弃
    local result = GetState() -- 返回一个新表
end

-- ✅ 复用表或采用对象池
local _tempPos = { x=0, y=0, z=0 }
function Update()
    _tempPos.x, _tempPos.y, _tempPos.z = 1, 2, 3
    SetPosition(_tempPos)
end
```

### 2. 字符串连接 `..`

- Lua 字符串不可变，`..` 每次都分配新字符串。
- 循环中拼接大量碎片 → **使用 `table.concat` 批量连接**。
- 频繁 `tostring()`、`string.format` 也会产新字符串。可缓存格式化模板，复用 buffer。

```lua
-- ❌ 热点：逐帧或循环内大量 ..
local str = ""
for i = 1, 100 do
    str = str .. i .. ","   -- 每次 .. 都生成新字符串
end

-- ✅ 优化：使用 table.concat
local t = {}
for i = 1, 100 do
    t[#t+1] = i
end
local str = table.concat(t, ",")
```

### 3. 函数闭包

- 在循环或每帧调用中定义函数，如 `table.sort(t, function(a,b) return a<b end)` → 每次分配新闭包。
- **提前定义函数并复用**。
- 闭包会捕获 upvalue，增加额外内存分配。

```lua
-- ❌ 每帧/循环内创建匿名函数
for _, btn in ipairs(buttons) do
    btn.onClick:AddListener(function() print("click") end)
end
-- Update 中定时器
Timer.Add(0.5, function() end)

-- ✅ 提升为具名函数或缓存
local function OnClick() print("click") end
for _, btn in ipairs(buttons) do
    btn.onClick:AddListener(OnClick)
end
```

### 4. C# 对象与静态成员的访问（xLua 最隐蔽的热点）

每次通过 `CS.UnityEngine.XXX` 都会涉及元表查找和缓存查询，生成临时值。最好把静态类引用缓存成 local。

```lua
-- ❌ 每帧通过 CS.xxx 访问，产生 userdata/临时查找表
function Update()
    local go = CS.UnityEngine.GameObject.Find("Player")
    local dt = CS.UnityEngine.Time.deltaTime
end

-- ✅ 缓存到 upvalue 或全局
local GameObject = CS.UnityEngine.GameObject
local Time = CS.UnityEngine.Time
function Update()
    local go = GameObject.Find("Player")
    local dt = Time.deltaTime
end
```

### 5. Unity 值类型（Vector3/Quaternion 等）

C# 对象成员（如 `transform.position`）每次返回 struct 副本（在 Lua 中体现为一个新 table），是**顶级热点**。

```lua
-- ❌ 读一次 position，生成一个表；修改后再赋值又生成表
local pos = transform.position   -- 产生新表
pos.x = pos.x + 1
transform.position = pos         -- 再次产生临时传递

-- ✅ 直接使用 Set 方法或缓存分量
local x = transform.position.x   -- 只取数字，无表
transform:SetPosition(x + 1, y, z)
-- 或 xLua 提供的 'ref' 传递 (需要生成代码支持)
transform:Translate(Vector3.forward) -- Vector3.forward 也是缓存表
```

频繁读写 Transform 的 position/rotation 是性能分析中占比最高的部分，务必用分量或 Set 方法替代。

### 6. 迭代器与泛型 for

```lua
-- ❌ pairs/ipairs 本身不产对象，但循环体内创建表、函数仍是热点
for k,v in pairs(t) do
    Process(k, v, { flag = true }) -- 每次迭代都建新表
end

-- ✅ 提前准备
local args = { flag = true }
for k,v in pairs(t) do
    args.flag = true -- 循环中仅��改
    Process(k, v, args)
end
```

### 7. 协程与 Update 中的高频调用

- `coroutine.create/wrap` 的协程体内部若产生临时表/闭包，每帧 resume 时会累积大量垃圾。
- 使用 `while true do ... coroutine.yield() end` 时避免每帧创建局部表或字符串。

### 8. 日志与 Debug

```lua
-- ❌ 每帧拼接字符串打印日志
print("pos:" .. pos.x .. "," .. pos.y)  -- 生产环境下字符串拼接徒增 GC
-- ✅ 尽量用条件输出或字符串池，或移除频繁日志
```

### 9. Lua 侧的委托与事件

```lua
-- ❌ 经常添加移除匿名函数
function OnEnable()
    button.onClick:AddListener(function() ... end)  -- 每次一个新闭包
end

-- ✅ 用成员方法并确保成对移除，避免泄漏的同时减少分配
function Class:OnClick() ... end
function Class:OnEnable()
    button.onClick:AddListener(self.OnClick)
end
```

### 10. 未缓存 LuaTable 索引器

```lua
-- ❌ 多次从同一个 LuaTable 取 C# 对象，每次都可能触发小对象分配
local a = tbl["key1"]
local b = tbl["key2"]  -- 若键对应 C# 对象，xLua 会从缓存获取，但仍有查找

-- ✅ 缓存引用
local cachedRef = tbl["key1"]
```

### 11. 循环引用

- 检查表之间的循环引用导致的内存泄漏。

### 12. 并发安全

- 网络消息处理考虑并发顺序、重入、防抖与上限。

### 13. 生命周期

- 协程/定时器是否存在泄漏（对象销毁后仍回调）。

### 优化工具与总结

- **Locus Lua GC 监控（Play Mode）**：在 Locus 中打开「Lua GC 监控」面板，或使用 Agent 工具 `lua_gc_analyze` 查看分配尖峰、GC 债务与泄漏风险告警。需先 `Locus.LuaGcBootstrap.Register(() => luaEnv)`。详见 `docs/lua-gc-monitor.md`。
- **使用 xLua Profiler**：通过 `XLua.LuaProfiler` 或 `LuaDLL` 接口导出内存快照，精确找出产生对象最多的代码行。
- **对象池与表复用**：对频繁使用的表（如坐标）实现池，`table.clear` 后重用。
- **避免"."触发元方法**：尽量 `local sin = math.sin` 后直接用。
- **升级 Lua 版本**：xLua 可配置 Lua 5.3/5.4，其增量 GC 对突发垃圾更宽容，但仍需保持良好编码习惯。

牢记：**缓存、复用、减少临时对象**，就能成倍降低 Lua GC 压力，让游戏帧率更平稳。
