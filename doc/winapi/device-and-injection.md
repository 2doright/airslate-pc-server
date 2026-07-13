# 设备创建与注入

## CreateSyntheticPointerDevice

定义于 `winuser.h`，用于创建指针注入设备，并声明应用后续允许同时注入的最大 contact 数量。

### 语法

```cpp
HSYNTHETICPOINTERDEVICE CreateSyntheticPointerDevice(
  POINTER_INPUT_TYPE    pointerType,
  ULONG                 maxCount,
  POINTER_FEEDBACK_MODE mode
);
```

### 参数

- `pointerType`
  - 指针注入设备类型。
  - 必须是 [PT_TOUCH](./input-types-and-limits.md) 或 [PT_PEN](./input-types-and-limits.md)。
- `maxCount`
  - 最大 contact 数量。
  - 对 [PT_TOUCH](./input-types-and-limits.md)，必须大于 0 且小于等于 [MAX_TOUCH_COUNT](./input-types-and-limits.md)。
  - 对 [PT_PEN](./input-types-and-limits.md)，必须为 1。
- `mode`
  - 接触可视化模式，常用值见 [输入类型与限制](./input-types-and-limits.md)。

### 返回值

- 成功时返回指针注入设备句柄。
- 失败时返回 `NULL`，可调用 `GetLastError` 获取扩展错误信息。

## InjectSyntheticPointerInput

定义于 `winuser.h`，用于把一组触摸或笔输入帧注入系统。

### 语法

```cpp
BOOL InjectSyntheticPointerInput(
  HSYNTHETICPOINTERDEVICE device,
  const POINTER_TYPE_INFO *pointerInfo,
  UINT32                  count
);
```

### 参数

- `device`
  - 由 [CreateSyntheticPointerDevice](./device-and-injection.md) 创建的指针注入设备句柄。
- `pointerInfo`
  - 待注入的 [POINTER_TYPE_INFO](./common-pointer-types.md) 数组。
  - 数组元素的实际类型必须与创建设备时使用的 `pointerType` 一致。
  - 每个元素中的 `ptPixelLocation` 以虚拟屏幕左上角为原点。
  - `GetMonitorInfoW` 返回的是桌面坐标；多屏虚拟桌面的左或上边界可能为负值。传入
    `ptPixelLocation` 前，必须分别减去 `SM_XVIRTUALSCREEN` 和 `SM_YVIRTUALSCREEN`，转换为
    虚拟屏幕左上角相对坐标。
- `count`
  - 本次注入的 contact 数量。
  - 对 [PT_TOUCH](./input-types-and-limits.md)，必须大于 0 且小于等于 [MAX_TOUCH_COUNT](./input-types-and-limits.md)。
  - 对 [PT_PEN](./input-types-and-limits.md)，必须为 1。

### 返回值

- 成功时返回 `TRUE`。
- 失败时返回 `FALSE`，可调用 `GetLastError` 获取扩展错误信息。

## 最小调用流程

1. 选择输入类型：触摸使用 [PT_TOUCH](./input-types-and-limits.md)，笔使用 [PT_PEN](./input-types-and-limits.md)。
2. 调用 [CreateSyntheticPointerDevice](./device-and-injection.md) 创建设备。
3. 填充 [POINTER_TYPE_INFO](./common-pointer-types.md)。
4. 根据类型补充 [POINTER_TOUCH_INFO](./touch.md) 或 [POINTER_PEN_INFO](./pen.md) 的专属字段。
5. 调用 [InjectSyntheticPointerInput](./device-and-injection.md) 注入一帧输入。
