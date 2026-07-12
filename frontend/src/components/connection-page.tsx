import type { AppBootstrapDto } from '../lib/tauri';
import { RefreshCw } from 'lucide-react';
import { setSelectedMonitor } from '../lib/tauri';
import { PressureCurveCard } from './pressure-curve-card';
import { Badge, Button, EmptyState, Panel, PanelHeader, SelectField } from './ui';

export function ConnectionPage(props: {
  data: AppBootstrapDto;
  selectedMonitor: AppBootstrapDto['monitors'][number] | null;
  busyKey: string | null;
  runAction: (key: string, action: () => Promise<unknown>) => Promise<void>;
  refreshingIpv4: boolean;
  onRefreshIpv4: () => void;
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
