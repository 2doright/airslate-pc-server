# 笔输入结构

## POINTER_PEN_INFO

`POINTER_PEN_INFO` 定义笔输入专属字段。

### 语法

```cpp
typedef struct tagPOINTER_PEN_INFO {
  POINTER_INFO pointerInfo;
  PEN_FLAGS    penFlags;
  PEN_MASK     penMask;
  UINT32       pressure;
  UINT32       rotation;
  INT32        tiltX;
  INT32        tiltY;
} POINTER_PEN_INFO;
```

### 成员

- `pointerInfo`
  - 公共指针头，定义见 [POINTER_INFO](./common-pointer-types.md)。
- `penFlags`
  - 笔标志，取值见 [PEN_FLAGS](./pen.md)。
- `penMask`
  - 笔掩码，决定哪些可选字段有效，取值见 [PEN_MASK](./pen.md)。
- `pressure`
  - 压力值，范围 `0` 到 `1024`，设备未报告时默认值为 `0`。
- `rotation`
  - 顺时针旋转角度，范围 `0` 到 `359`。
- `tiltX`
  - 沿 x 轴的倾斜角度，范围 `-90` 到 `+90`。
- `tiltY`
  - 沿 y 轴的倾斜角度，范围 `-90` 到 `+90`。

## PEN_FLAGS

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `PEN_FLAG_NONE` | `0x00000000` | 默认值。 |
| `PEN_FLAG_BARREL` | `0x00000001` | 桶按钮按下。 |
| `PEN_FLAG_INVERTED` | `0x00000002` | 笔处于倒置状态。 |
| `PEN_FLAG_ERASER` | `0x00000004` | 橡皮擦按钮按下。 |

## PEN_MASK

`PEN_MASK` 表示哪些可选字段包含有效值。

| 常量 | 值 | 说明 |
| --- | --- | --- |
| `PEN_MASK_NONE` | `0x00000000` | 默认值，表示没有可选字段有效。 |
| `PEN_MASK_PRESSURE` | `0x00000001` | `pressure` 有效。 |
| `PEN_MASK_ROTATION` | `0x00000002` | `rotation` 有效。 |
| `PEN_MASK_TILT_X` | `0x00000004` | `tiltX` 有效。 |
| `PEN_MASK_TILT_Y` | `0x00000008` | `tiltY` 有效。 |

## 实现提示

- 笔输入的公共状态位位于 [POINTER_FLAGS](./common-pointer-types.md)。
- 只有在对应字段实际提供值时，才应在 [PEN_MASK](./pen.md) 中标记该字段有效。
- 笔设备只能以单 contact 方式注入，相关约束见 [输入类型与限制](./input-types-and-limits.md)。
