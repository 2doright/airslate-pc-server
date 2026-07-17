# USBAccessory 正式有线会话（PC Host）

本模块是 PC Server 的正式传输之一，与 TCP 握手/UDP 数据面共享 `HandshakeService`、
`SessionService`、`SessionLifecycle` 和 `StylusInputPipeline`，不维护第二套会话或输入状态。

## OpenHarmony / HarmonyOS 官方依据

- HarmonyOS `@ohos.usbManager` 的 `USBAccessory`、授权和 `openAccessory`：
  <https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-usbmanager>
- OpenHarmony 官方 `usb_usb_manager` 固定提交中的设备侧 accessory 管理实现：
  <https://github.com/openharmony/usb_usb_manager/blob/d516eeae311ab0e3d0da5e4a07a1a087b4cc53d6/services/native/src/usb_accessory_manager.cpp>
- 同一官方提交定义的 accessory 身份字段：
  <https://github.com/openharmony/usb_usb_manager/blob/d516eeae311ab0e3d0da5e4a07a1a087b4cc53d6/interfaces/innerkits/native/include/usb_accessory.h>
- PC/平板正式字节契约以 `E:\Personal\AirSlate\doc\server.md` 第 3～7 节为准。

endpoint-zero 协商常量来自 OpenHarmony 官方设备侧 accessory 实现：device-recipient vendor
`GET_PROTOCOL` request 51（IN，value/index 0，2 字节小端）、`SEND_STRING` request 52
（OUT，value 0、index 为身份槽位、NUL 结尾 UTF-8）、`START` request 53（OUT，value/index 0）。
正式产品固定发送：

- index 0 manufacturer：`AirSlate`
- index 1 product：`AirSlate PC Server`
- index 2 description：`AirSlate formal wired session`
- index 3 version：当前 Cargo 包版本
- index 4 URI：AirSlate PC Server 项目地址
- index 5 serial：`AirSlate-PC`

鸿蒙正式版只用前两项选择配件；PC 端不得改变这两个值。实现和文档不采用其他平台 API 或
示例作为规范。

## 枚举、WinUSB 与物理设备绑定

正式服务的初始路径只对唯一的 Harmony accessory-compatible 接口形态 `FF/50/01` 发起协商；出现多个候选即停止，绝不向
所有 USB 设备盲发 vendor request。`START` 前通过 Windows SetupAPI 记录该 devnode 的完整
`DEVPKEY_Device_LocationPaths`，之后只接受同一物理 LocationPath 上的 `FF/FF/00` accessory
function。VID/PID、端点地址和 max packet size 均不硬编码。

如果初始 `FF/50/01` 接口暂不可见，但枚举中存在唯一的 `FF/FF/00` accessory function，PC 会先
打开该设备并从真实描述符确认只有一组 Bulk IN/OUT；确认成功后直接进入同一正式 Bulk session，
不发送 endpoint-zero vendor request。多个 accessory function、无 Bulk pair 或 Bulk pair 不唯一
都会拒绝猜测并继续报告诊断状态。

若重枚举实例为 Code 28，正式进程仅在同时满足下列事实时请求一次 UAC：present、Service 为空、
Compatible ID 精确包含 `USB\Class_FF&SubClass_FF&Prot_00`、LocationPath 精确相同、目标关联
driver list 中恰有一个 Microsoft `%SystemRoot%\inf\winusb.inf` / `WINUSB` / `USBDevice`
节点。提权后的同一程序重新核验全部事实，调用 `SetupDiSetSelectedDriver` 和 `DiInstallDevice`，
写入 AirSlate interface GUID。流程不使用 Zadig、自定义 INF 或 UsbDk。

上述 WinUSB 事实核验和一次性 UAC 安装只作为正式 USB 服务的内部步骤执行；项目不再发布或运行
独立的 dry-run 二进制，也不会要求用户手动安装驱动。

## 正式 ASLT 字节流

Bulk 打开后的 USB 私有启动顺序固定为：PC→平板 8 字节 `USB_READY` → 平板→PC 72 字节
`HANDSHAKE_REQUEST` → PC→平板 81 字节 `HANDSHAKE_RESPONSE` 或 108 字节
`HANDSHAKE_ERROR`。`USB_READY` 的字节布局是 magic `0x41534C54` 小端、type `7`、version `1`、
reserved `u16 = 0`；它只用于 USB transport bootstrap，不进入通用正式会话解析，因此无线仍直接
以 `HANDSHAKE_REQUEST` 开始。成功后同一 Bulk 会话只接受 72 字节 `SESSION_DISCONNECT`、
36 字节 `STYLUS_FRAME` 和 36 字节 `GESTURE_FRAME`。

流解析按 magic/type 决定固定长度，处理短读和粘包；写端使用 `write_all` 处理短写。未知 magic、
未知 type、固定长度内容校验失败、握手阶段包型错误都会立即终止当前 USB 会话，不扫描恢复。
手写与手势帧通过 USB connection id 校验同一活动会话后，进入与无线相同的输入注入链路；两类
帧的 `seq` 继续由既有输入/快捷键处理逻辑共同观察。正常 disconnect、EOF、拔线和 WinUSB I/O
错误都会运行相同的会话清理，并通过 `usb-status-changed` 向 UI 报告事实。

平板尚未完成用户授权时，`USB_READY` 的 Bulk OUT 单次 transfer 可能 timeout 或 STALL。PC 对
timeout 以 100 ms 退避重试，并按 `Completion::actual_len` 从未送达的偏移继续，避免短写后重复
帧前缀；STALL 则在该次 raw transfer 已经完成/取消、没有 pending transfer 后调用
`Endpoint::clear_halt()` 再退避重试。nusb 0.2.4 的 Windows backend 将它实现为
`WinUsb_ResetPipe`。只有 8 字节全部实际传输后才等待 `HANDSHAKE_REQUEST`；拔线和其他真实错误
立即终止。

nusb 0.2.4 还会把 WinUSB 提交阶段的 Windows `ERROR_BAD_COMMAND`（OS error 22）映射成
`TransferError::Disconnected`。由于该映射同时覆盖真实拔线，PC 只在 USB_READY 尚未实际发送任何
字节、且同一总线/端口和 Windows LocationPath 仍可枚举时，短暂关闭并重新打开动态 Bulk 句柄，最多
6 次；部分短写、LocationPath 消失/改变、权限或驱动错误不会重发或吞掉。

握手前的 Bulk IN 同样使用 nusb 0.2.4 单次 transfer；收到 `TransferError::Stall` 后按上述
无 pending transfer 前提执行 `clear_halt` 并继续等待同一连接。正式握手成功后改用流式 reader，
此后的 STALL 是活动会话真实失败，必须终止并清理，不能无限吞掉。

## UI 状态与操作

连接页的“有线连接”面板直接显示 `usb-status-changed` 的最新事实快照；启动时通过应用
bootstrap 读取同一快照，避免窗口打开晚于 USB 服务时显示过期的默认状态。状态含义为：

- `waiting`：服务正在枚举，尚未找到唯一可用配件；普通 WPD/MTP 文件传输不会被当作控制通道。
- `waiting_accessory`：已从真实描述符识别平板文件传输接口，但 Windows 当前没有提供可供
  endpoint-zero 配件协商的 WinUSB 初始接口；面板提示在平板 AirSlate 的有线 USB 页发起
  连接并授权，完成后点击重试。该状态不把文件传输端点提升为 raw 会话。
- `authorizing`：已经找到唯一初始配件接口，正在等待用户授权或执行官方 accessory 协商。
- `handshaking`：已收到完整的 72 字节 `HANDSHAKE_REQUEST`，正在执行正式会话握手并回写响应。
- `connected`：72 字节请求已通过正式会话逻辑校验并完成响应写入；只有此状态才允许输入帧进入
  共享会话/注入链路。
- `error`：真实枚举、授权、驱动、协议或 I/O 错误；面板保留最后一次真实设备描述符事实，并
  提供有界重试。

面板中的 VID/PID、物理 bus/port、interface/alternate、Bulk 地址和 max packet size 都由 nusb
描述符产生，不是 UI 配置项。重试只请求服务立即重新扫描并在活动 USB 会话中执行同一清理；
断开按钮复用正式会话的本地清理，不发送伪造的协议包。

## 运行与双端联调

开发运行：

```powershell
cargo run --bin airslate_pc_server
```

打开鸿蒙端正式有线模式后插入数据线。首次出现 Code 28 时确认 AirSlate 发起的一次 Windows UAC；
若 Windows 返回需要重启，先重启，否则按提示在同一物理口重插。PC 日志应依次打印协商、真实
configuration/interface/alternate/Bulk IN/OUT/max packet、`delivered USB_READY`、正式握手和连接
状态。只有 `USB_READY` 完整传输、收到合法 `HANDSHAKE_REQUEST` 并回写成功响应后 UI 才显示
活动会话；枚举或打开 Bulk 不等同于连接成功。

每次 USB 会话 I/O 返回后，服务会明确记录 nusb 句柄已释放、会话状态已清理并回到初始
accessory-compatible 扫描。
扫描日志只在可见设备/描述符状态改变时输出，包含 USB VID/PID、物理 bus/port 和 interface
class/subclass/protocol 摘要。普通 MTP/WPD（例如仅 `wpdmtp.inf`、没有 `FF/50/01` 初始接口）
不是控制通道，不会被当作候选；必须先让平板暴露可由 WinUSB/nusb 打开的 accessory function，PC
才会发送 endpoint-zero 协商请求。
