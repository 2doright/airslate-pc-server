# 公共指针结构与标志

## POINTER_TYPE_INFO

`POINTER_TYPE_INFO` 是注入时使用的顶层结构，按类型承载公共字段和触摸/笔专属字段。

### 语法

```cpp
typedef struct tagPOINTER_TYPE_INFO {
  POINTER_INPUT_TYPE type;
  union {
    POINTER_INFO       pointerInfo;
    POINTER_TOUCH_INFO touchInfo;
    POINTER_PEN_INFO   penInfo;
  } DUMMYUNIONNAME;
} POINTER_TYPE_INFO, *PPOINTER_TYPE_INFO;
```

### 成员

- `type`
  - 指针输入类型，取值见 [POINTER_INPUT_TYPE](./input-types-and-limits.md)。
- `pointerInfo`
  - 公共指针信息，定义见 [POINTER_INFO](./common-pointer-types.md)。
- `touchInfo`
  - 触摸输入专属信息，定义见 [POINTER_TOUCH_INFO](./touch.md)。
- `penInfo`
  - 笔输入专属信息，定义见 [POINTER_PEN_INFO](./pen.md)。

## POINTER_INFO

`POINTER_INFO` 包含所有指针类型通用的基础字段。

### 语法

```cpp
typedef struct tagPOINTER_INFO {
  POINTER_INPUT_TYPE         pointerType;
  UINT32                     pointerId;
  UINT32                     frameId;
  POINTER_FLAGS              pointerFlags;
  HANDLE                     sourceDevice;
  HWND                       hwndTarget;
  POINT                      ptPixelLocation;
  POINT                      ptHimetricLocation;
  POINT                      ptPixelLocationRaw;
  POINT                      ptHimetricLocationRaw;
  DWORD                      dwTime;
  UINT32                     historyCount;
  INT32                      InputData;
  DWORD                      dwKeyStates;
  UINT64                     PerformanceCount;
  POINTER_BUTTON_CHANGE_TYPE ButtonChangeType;
} POINTER_INFO;
```

### 关键字段

| 字段 | 说明 |
| --- | --- |
| `pointerType` | 指针类型，取值见 [POINTER_INPUT_TYPE](./input-types-and-limits.md)。 |
| `pointerId` | 在指针生命周期内唯一标识一个 contact。 |
| `frameId` | 标识同一输入帧中的多个 contact。 |
| `pointerFlags` | 指针状态位，取值见下方 [POINTER_FLAGS](./common-pointer-types.md)。 |
| `ptPixelLocation` | 预测后的屏幕像素坐标，原点为虚拟屏幕左上角。 |
| `ptPixelLocationRaw` | 原始屏幕像素坐标。 |
| `dwTime` | 基于系统时钟的时间戳。 |
| `dwKeyStates` | 生成输入时的修饰键状态。 |
| `PerformanceCount` | 高精度性能计数器时间戳。 |
| `ButtonChangeType` | 与上一个输入之间的按钮变化类型。 |

### 时间戳约束

- `dwTime` 和 `PerformanceCount` 都可用于注入时间戳。
- 同一组注入流程中应保持时间戳策略一致，不要中途切换。
- 如果时间戳窗口重复，系统可能返回 `ERROR_NOT_READY`。

## POINTER_FLAGS

`POINTER_FLAGS` 用于描述 contact 的状态和转换。

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `POINTER_FLAG_NONE` | `0x00000000` | 默认值。 |
| `POINTER_FLAG_NEW` | `0x00000001` | 新指针到达。 |
| `POINTER_FLAG_INRANGE` | `0x00000002` | 指针仍处于检测范围内。 |
| `POINTER_FLAG_INCONTACT` | `0x00000004` | 指针与数字化器表面接触。 |
| `POINTER_FLAG_FIRSTBUTTON` | `0x00000010` | 主按钮状态，触摸接触时通常会设置。 |
| `POINTER_FLAG_SECONDBUTTON` | `0x00000020` | 次按钮状态。 |
| `POINTER_FLAG_THIRDBUTTON` | `0x00000040` | 第三按钮状态。 |
| `POINTER_FLAG_FOURTHBUTTON` | `0x00000080` | 第四按钮状态。 |
| `POINTER_FLAG_FIFTHBUTTON` | `0x00000100` | 第五按钮状态。 |
| `POINTER_FLAG_PRIMARY` | `0x00002000` | 当前主指针。 |
| `POINTER_FLAG_CONFIDENCE` | `0x00004000` | 设备对该输入是预期交互有较高置信度。 |
| `POINTER_FLAG_CANCELED` | `0x00008000` | 指针异常离开，交互应视为取消。 |
| `POINTER_FLAG_DOWN` | `0x00010000` | 指针进入按下状态。 |
| `POINTER_FLAG_UPDATE` | `0x00020000` | 指针更新，但未发生状态切换。 |
| `POINTER_FLAG_UP` | `0x00040000` | 指针进入抬起状态。 |
| `POINTER_FLAG_WHEEL` | `0x00080000` | 滚轮输入。 |
| `POINTER_FLAG_HWHEEL` | `0x00100000` | 水平滚轮输入。 |
| `POINTER_FLAG_CAPTURECHANGED` | `0x00200000` | 指针捕获目标发生变化。 |
| `POINTER_FLAG_HASTRANSFORM` | `0x00400000` | 指针带有关联变换。 |

## POINT

`POINT` 用于表示二维点坐标。

### 语法

```cpp
typedef struct tagPOINT {
  LONG x;
  LONG y;
} POINT;
```

### 成员

- `x`：点的 x 坐标。
- `y`：点的 y 坐标。

### 说明

- `POINT` 与 `POINTL` 结构等价。
- 在本目录文档里，最常见的用法是配合 [POINTER_INFO](./common-pointer-types.md) 表示屏幕坐标。
