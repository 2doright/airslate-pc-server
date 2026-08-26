import type { AppBootstrapDto, UsbStatusEvent } from '../lib/tauri';
import { Cable, CheckCircle2, CircleAlert, CircleHelp, RefreshCw } from 'lucide-react';
import { setSelectedMonitor } from '../lib/tauri';
import { PressureCurveCard } from './pressure-curve-card';
import { Badge, Button, EmptyState, Panel, PanelHeader, SelectField, Switch } from './ui';

export function ConnectionPage(props: {
  data: AppBootstrapDto;
  usbStatus: UsbStatusEvent;
  selectedMonitor: AppBootstrapDto['monitors'][number] | null;
  busyKey: string | null;
  runAction: (key: string, action: () => Promise<unknown>) => Promise<void>;
  refreshingIpv4: boolean;
  onRefreshIpv4: () => void;
  onRetryUsb: () => void;
  onSetWiredConnectionEnabled: (enabled: boolean) => void;
}) {
  const primaryAddress = props.data.ipv4Values[0] ?? null;
  const secondaryAddresses = props.data.ipv4Values.slice(1);

  return (
    <div className="connection-grid">
      <Panel variant="hero" className="connect-hero">
        <PanelHeader
          title="局域网 IPv4 地址"
          action={(
            <Button
              type="button"
              tone="ghost"
              className="ipv4-refresh-button"
              onClick={props.onRefreshIpv4}
              disabled={props.refreshingIpv4}
              aria-label="刷新局域网 IPv4 地址"
              title="刷新局域网 IPv4 地址"
            >
              <RefreshCw className={props.refreshingIpv4 ? 'shell-lucide-icon shell-lucide-icon--small shell-lucide-icon--spinning' : 'shell-lucide-icon shell-lucide-icon--small'} />
            </Button>
          )}
        />
        {primaryAddress ? (
          <div className="ip-showcase">
            <div className="ip-showcase__value">{primaryAddress}</div>
            {secondaryAddresses.length > 0 ? (
              <div className="ip-secondary-list" aria-label="其他局域网地址">
                {secondaryAddresses.map((value) => (
                  <span key={value} className="ip-secondary">{value}</span>
                ))}
              </div>
            ) : null}
          </div>
        ) : (
          <EmptyState title="未发现局域网地址">检查网络连接后刷新。</EmptyState>
        )}
      </Panel>

      <UsbConnectionPanel
        status={props.usbStatus}
        enabled={props.data.wiredConnectionEnabled}
        retryBusy={props.busyKey === 'usb-retry' || props.busyKey === 'wired-connection'}
        toggleBusy={props.busyKey === 'wired-connection' || props.busyKey === 'usb-retry'}
        onRetry={props.onRetryUsb}
        onSetEnabled={props.onSetWiredConnectionEnabled}
      />

      <Panel className="monitor-card">
        <PanelHeader title="显示器" action={props.selectedMonitor?.isPrimary ? <Badge tone="accent">主屏幕</Badge> : null} />
        <label className="monitor-summary">
          <span className="monitor-summary__content">
            <span className="monitor-summary__name">{props.selectedMonitor?.label ?? '未发现显示器'}</span>
          </span>
          <SelectField
            value={props.data.monitors.find((monitor) => monitor.selected)?.id ?? ''}
            disabled={props.busyKey === 'monitor'}
            ariaLabel="选择显示器"
            options={props.data.monitors.map((monitor) => ({
              value: monitor.id,
              label: `${monitor.label} · ${monitor.pixelWidth}×${monitor.pixelHeight}`,
            }))}
            onChange={(value) => {
              void props.runAction('monitor', () => setSelectedMonitor(value));
            }}
          />
        </label>
      </Panel>

      <PressureCurveCard curve={props.data.pressureCurve} busy={props.busyKey === 'pressure'} runAction={props.runAction} />
    </div>
  );
}

function UsbConnectionPanel(props: {
  status: UsbStatusEvent;
  enabled: boolean;
  retryBusy: boolean;
  toggleBusy: boolean;
  onRetry: () => void;
  onSetEnabled: (enabled: boolean) => void;
}) {
  const state = props.status.state;
  const statusLabel = usbStatusLabel(state);
  const statusCopy = usbStatusCopy(props.status);
  const statusTone = state === 'connected' ? 'success' : state === 'error' ? 'warning' : 'accent';

  return (
    <Panel variant="hero" className="usb-panel">
      <PanelHeader
        title="有线连接"
        action={(
          <>
            <Switch
              checked={props.enabled}
              disabled={props.toggleBusy || (props.enabled && state === 'connected')}
              ariaLabel="启用或关闭有线连接"
              onChange={props.onSetEnabled}
            />
            {props.enabled ? (
              <>
                <Badge tone={statusTone}>{statusLabel}</Badge>
                {props.status.retryable ? (
                  <Button
                    type="button"
                    tone="ghost"
                    className="ipv4-refresh-button"
                    onClick={props.onRetry}
                    disabled={props.retryBusy}
                    aria-label="刷新有线连接"
                    title="刷新有线连接"
                  >
                    <RefreshCw className={props.retryBusy ? 'shell-lucide-icon shell-lucide-icon--small shell-lucide-icon--spinning' : 'shell-lucide-icon shell-lucide-icon--small'} />
                  </Button>
                ) : null}
              </>
            ) : null}
            <span
              className="usb-panel__help"
              role="img"
              aria-label="USB 接口配置说明"
              title="无法正常连接时，请前往设置 → 通用 → USB 设备接口，获取当前鸿蒙设备的接口值并填写。"
            >
              <CircleHelp aria-hidden="true" />
            </span>
          </>
        )}
      />
      {props.enabled ? (
        <div className="usb-panel__status" role="status" aria-live="polite">
          <div className="usb-panel__status-icon" data-state={state}>
            {state === 'connected' ? <CheckCircle2 /> : state === 'error' ? <CircleAlert /> : <Cable />}
          </div>
          <div className="usb-panel__status-copy">
            <strong>{statusCopy.title}</strong>
            <span title={props.status.detail}>{statusCopy.detail}</span>
          </div>
        </div>
      ) : null}
    </Panel>
  );
}

function usbStatusLabel(state: UsbStatusEvent['state']) {
  switch (state) {
    case 'waiting_accessory': return '等待授权';
    case 'authorizing': return '等待授权';
    case 'handshaking': return '正在连接';
    case 'connected': return '已连接';
    case 'error': return '连接失败';
    default: return '未连接';
  }
}

function usbStatusCopy(status: UsbStatusEvent) {
  switch (status.state) {
    case 'waiting_accessory':
      return { title: '等待授权', detail: '请在平板的 AirSlate 页面发起连接并授权' };
    case 'authorizing':
      return { title: '等待授权', detail: '请在平板上允许 AirSlate 访问 USB' };
    case 'handshaking':
      return { title: '正在连接', detail: '正在与平板建立有线会话' };
    case 'connected':
      return { title: '已连接', detail: '有线连接已就绪，可以使用' };
    case 'error':
      return { title: '连接失败', detail: '请重新插拔 USB 数据线后重试' };
    default:
      return { title: '未连接', detail: '连接平板后会自动开始' };
  }
}
