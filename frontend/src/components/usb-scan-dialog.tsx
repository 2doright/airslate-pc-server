import { useEffect, useRef, useState } from 'react';
import { RefreshCw, X } from 'lucide-react';
import { scanUsbDevices, type UsbScanDevice } from '../lib/tauri';

const USB_SCAN_INTERVAL_MS = 750;

export function UsbScanDialog(props: { onClose: () => void }) {
  const [devices, setDevices] = useState<UsbScanDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);
  const [waitingForInterfacePair, setWaitingForInterfacePair] = useState(false);
  const baselineKeys = useRef<Set<string> | null>(null);
  const trackedDevices = useRef(new Map<string, UsbScanDevice>());

  useEffect(() => {
    let disposed = false;
    let inFlight = false;
    let hasSnapshot = false;

    const scan = async () => {
      if (inFlight) return;
      inFlight = true;
      if (!hasSnapshot) setLoading(true);

      try {
        const hdcDevices = (await scanUsbDevices()).filter(isHdcDevice);
        const nextDevices = hdcDevices.filter(hasInitialAndCurrentInterfaces);
        if (disposed) return;
        setWaitingForInterfacePair(hdcDevices.length > 0 && nextDevices.length === 0);

        const currentKeys = new Set(nextDevices.map(usbPhysicalKey));
        let baseline = baselineKeys.current;
        if (baseline === null) {
          baseline = currentKeys;
          baselineKeys.current = baseline;
        } else {
          for (const key of baseline) {
            if (!currentKeys.has(key)) baseline.delete(key);
          }
        }

        for (const device of nextDevices) {
          const key = usbPhysicalKey(device);
          if (!baseline.has(key)) {
            trackedDevices.current.set(key, device);
          }
        }
        for (const key of trackedDevices.current.keys()) {
          if (!currentKeys.has(key)) trackedDevices.current.delete(key);
        }

        setDevices([...trackedDevices.current.values()]);
        setError(null);
        hasSnapshot = true;
      } catch (scanError) {
        if (disposed) return;
        setWaitingForInterfacePair(false);
        setError(scanError instanceof Error ? scanError.message : String(scanError));
        hasSnapshot = true;
      } finally {
        inFlight = false;
        if (!disposed) setLoading(false);
      }
    };

    void scan();
    const timer = window.setInterval(() => void scan(), USB_SCAN_INTERVAL_MS);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [refreshToken]);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') props.onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [props.onClose]);

  return (
    <div
      className="usb-scan-dialog"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <section
        className="usb-scan-dialog__panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="usb-scan-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="usb-scan-dialog__header">
          <div>
            <h2 id="usb-scan-dialog-title">扫描 USB 设备</h2>
            <p>自动识别名称为 HDC Device 的设备；完成两阶段识别后显示原始识别接口。</p>
          </div>
          <div className="usb-scan-dialog__header-actions">
            <button
              type="button"
              className="usb-scan-dialog__close"
              aria-label="立即刷新"
              onClick={() => setRefreshToken((value) => value + 1)}
              disabled={loading}
            >
              <RefreshCw aria-hidden="true" />
            </button>
            <button type="button" className="usb-scan-dialog__close" aria-label="关闭扫描窗口" onClick={props.onClose}>
              <X aria-hidden="true" />
            </button>
          </div>
        </header>

        <div className="usb-scan-dialog__status" role="status" aria-live="polite">
          <span className={error ? 'usb-scan-dialog__status-dot usb-scan-dialog__status-dot--error' : 'usb-scan-dialog__status-dot'} />
          {error
            ? '扫描失败'
            : waitingForInterfacePair && devices.length === 0
              ? '持续检测中 · 等待完成两阶段识别'
              : `持续检测中 · 已发现 ${devices.length} 个识别接口`}
        </div>

        <div className="usb-scan-dialog__body">
          {error ? <div className="usb-scan-dialog__error">{error}</div> : null}
          {!loading && !error && devices.length === 0 ? (
            <div className="usb-scan-dialog__empty">
              <strong>{waitingForInterfacePair ? 'HDC Device 尚未完成两阶段识别' : '暂未检测到新的 HDC Device 设备'}</strong>
              <span>{waitingForInterfacePair ? '请重新插入设备，直至出现识别接口。' : '保持此窗口打开，然后插入鸿蒙设备。'}</span>
            </div>
          ) : null}
          <div className="usb-scan-dialog__devices">
            {devices.map((device, index) => (
              <UsbScanDeviceCard key={usbPhysicalKey(device)} device={device} index={index} />
            ))}
          </div>
        </div>

      </section>
    </div>
  );
}

function UsbScanDeviceCard(props: { device: UsbScanDevice; index: number }) {
  const { device } = props;
  const initialInterfaces = device.initialInterfaces ?? device.interfaces;

  return (
    <article className="usb-scan-device-card">
      <header className="usb-scan-device-card__header">
        <div>
          <strong>{formatDeviceName(device, props.index)}</strong>
        </div>
      </header>
      {initialInterfaces.length > 0 ? (
        <div className="usb-scan-interface-groups">
          <UsbScanInterfaceGroup label="识别接口" interfaces={initialInterfaces} />
        </div>
      ) : (
        <div className="usb-scan-device-card__empty">系统当前未提供接口描述。</div>
      )}
    </article>
  );
}

function UsbScanInterfaceGroup(props: { label: string; interfaces: UsbScanDevice['interfaces'] }) {
  return (
    <section className="usb-scan-interface-group">
      <strong className="usb-scan-interface-group__label">{props.label}</strong>
      {props.interfaces.length > 0 ? (
        <div className="usb-scan-interface-list">
          {props.interfaces.map((usbInterface) => (
            <div className="usb-scan-interface" key={`${usbInterface.interfaceNumber}-${usbInterface.classCode}-${usbInterface.subclass}-${usbInterface.protocol}`}>
              <code>{formatInterfaceCompact(usbInterface)}</code>
            </div>
          ))}
        </div>
      ) : (
        <div className="usb-scan-device-card__empty">系统当前未提供接口描述。</div>
      )}
    </section>
  );
}

function formatInterfaceCompact(usbInterface: UsbScanDevice['interfaces'][number]) {
  return [usbInterface.classCode, usbInterface.subclass, usbInterface.protocol]
    .map((value) => formatHex(value, 2))
    .join('');
}

function formatDeviceName(device: UsbScanDevice, index: number) {
  const name = [
    device.initialProduct,
    device.product,
    device.initialManufacturer,
    device.manufacturer,
  ].find((value): value is string => Boolean(value));
  return name ?? `USB 设备 ${index + 1}`;
}

function isHdcDevice(device: UsbScanDevice) {
  return [device.product, device.initialProduct]
    .some((name) => name?.trim().toLowerCase() === 'hdc device');
}

function hasInitialAndCurrentInterfaces(device: UsbScanDevice) {
  const initialInterfaces = device.initialInterfaces;
  return initialInterfaces !== null
    && initialInterfaces.length > 0
    && device.interfaces.length > 0
    && !sameInterfaces(initialInterfaces, device.interfaces);
}

function sameInterfaces(left: UsbScanDevice['interfaces'], right: UsbScanDevice['interfaces']) {
  return left.length === right.length && left.every((leftInterface, index) => {
    const rightInterface = right[index];
    return rightInterface !== undefined
      && leftInterface.interfaceNumber === rightInterface.interfaceNumber
      && leftInterface.classCode === rightInterface.classCode
      && leftInterface.subclass === rightInterface.subclass
      && leftInterface.protocol === rightInterface.protocol;
  });
}

function usbPhysicalKey(device: UsbScanDevice) {
  return `${device.busId}\u0000${device.portChain.join('.')}`;
}

function formatHex(value: number, width: number) {
  return value.toString(16).padStart(width, '0').toUpperCase();
}
