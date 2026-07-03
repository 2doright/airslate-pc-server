import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { ConnectionPage } from './components/connection-page';
import { Shell } from './components/shell';
import { ShortcutsPage, type RecordingTarget } from './components/shortcuts-page';
import { Button, EmptyState, Panel, Switch } from './components/ui';
import { type AppBootstrapDto, getAppBootstrap, setBindingKeys, setLaunchAtStartup, setRadialOuterSlot } from './lib/tauri';

type PageKey = 'connection' | 'shortcuts';

function IconFrame(props: { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {props.children}
    </svg>
  );
}

function WirelessIcon() {
  return (
    <IconFrame>
      <path d="M4.93 9.93a10 10 0 0 1 14.14 0" />
      <path d="M7.76 12.76a6 6 0 0 1 8.48 0" />
      <path d="M10.59 15.59a2 2 0 0 1 2.82 0" />
      <circle cx="12" cy="18" r="1" fill="currentColor" stroke="none" />
    </IconFrame>
  );
}

function ShortcutIcon() {
  return (
    <IconFrame>
      <rect x="4" y="6" width="16" height="12" rx="3" />
      <path d="M8 10h.01" />
      <path d="M12 10h.01" />
      <path d="M16 10h.01" />
      <path d="M8 14h8" />
    </IconFrame>
  );
}

function RefreshIcon() {
  return (
    <IconFrame>
      <path d="M21 12a9 9 0 1 1-2.64-6.36" />
      <path d="M21 3v6h-6" />
    </IconFrame>
  );
}

export function App() {
  const [page, setPage] = useState<PageKey>('connection');
  const [data, setData] = useState<AppBootstrapDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [newPresetName, setNewPresetName] = useState('');
  const [recordingTarget, setRecordingTarget] = useState<RecordingTarget | null>(null);

  const load = async (options?: { preserveView?: boolean }) => {
    const preserveView = options?.preserveView ?? false;
    if (preserveView) {
      setRefreshing(true);
    } else {
      setLoading(true);
      setError(null);
    }
    try {
      const next = await getAppBootstrap();
      setData(next);
      if (preserveView) setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (preserveView) {
        setRefreshing(false);
      } else {
        setLoading(false);
      }
      setBusyKey(null);
    }
  };

  const runAction = async (key: string, action: () => Promise<unknown>) => {
    setBusyKey(key);
    try {
      await action();
      await load({ preserveView: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusyKey(null);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  useEffect(() => {
    if (!recordingTarget) return;
    const pressed = new Set<string>();

    const commit = (keys: string[]) => {
      const target = recordingTarget;
      const normalized = normalizeRecordedKeys(keys);
      if (normalized.length === 0) return;
      setRecordingTarget(null);
      void runAction(target.busyKey, () =>
        target.kind === 'binding' ? setBindingKeys(target.bindingId, normalized) : setRadialOuterSlot(target.index, normalized),
      );
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.repeat) return;
      if (event.key === 'Escape') {
        setRecordingTarget(null);
        return;
      }
      const mapped = mapKeyboardEventToKey(event);
      if (mapped) pressed.add(mapped);
    };

    const handleKeyUp = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'Escape') {
        setRecordingTarget(null);
        return;
      }
      const mapped = mapKeyboardEventToKey(event);
      if (!mapped) return;
      pressed.add(mapped);
      commit([...pressed]);
    };

    window.addEventListener('keydown', handleKeyDown, true);
    window.addEventListener('keyup', handleKeyUp, true);
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true);
      window.removeEventListener('keyup', handleKeyUp, true);
    };
  }, [recordingTarget]);

  const nav = useMemo(
    () => [
      { key: 'connection', label: '连接', icon: <WirelessIcon />, active: page === 'connection', onClick: () => setPage('connection') },
      { key: 'shortcuts', label: '快捷键', icon: <ShortcutIcon />, active: page === 'shortcuts', onClick: () => setPage('shortcuts') },
    ],
    [page],
  );

  const selectedMonitor = data?.monitors.find((monitor) => monitor.selected) ?? data?.monitors[0] ?? null;

  return (
    <Shell
      appName="AirSlate"
      nav={nav}
      meta={null}
      navAccessory={
        data ? (
          <div className="startup-toggle">
            <div className="startup-toggle__copy">
              <div className="startup-toggle__label">开机启动</div>
              <div className="startup-toggle__hint">自动启动时仅驻留系统托盘</div>
            </div>
            <Switch
              checked={data.launchAtStartup}
              disabled={loading || refreshing || busyKey === 'launch-at-startup'}
              ariaLabel="切换开机启动"
              onChange={(enabled) => void runAction('launch-at-startup', () => setLaunchAtStartup(enabled))}
            />
          </div>
        ) : null
      }
      actions={
        <Button type="button" tone="ghost" onClick={() => void load({ preserveView: Boolean(data) })} disabled={loading || refreshing} aria-label="刷新">
          <RefreshIcon />
        </Button>
      }
    >
      {loading && <LoadingState />}
      {!loading && error && !data && <ErrorState error={error} onRetry={() => void load()} />}
      {!loading && data && (
        <div className="page-stack">
          {error ? <ErrorState error={error} onRetry={() => void load({ preserveView: true })} /> : null}
          {page === 'connection' ? (
            <ConnectionPage data={data} selectedMonitor={selectedMonitor} busyKey={busyKey} runAction={runAction} />
          ) : (
            <ShortcutsPage
              data={data}
              busyKey={busyKey}
              newPresetName={newPresetName}
              setNewPresetName={setNewPresetName}
              runAction={runAction}
              recordingTarget={recordingTarget}
              setRecordingTarget={setRecordingTarget}
            />
          )}
        </div>
      )}
    </Shell>
  );
}

function LoadingState() {
  return (
    <Panel>
      <EmptyState title="正在读取应用状态" />
    </Panel>
  );
}

function ErrorState(props: { error: string; onRetry: () => void }) {
  return (
    <Panel className="error-panel">
      <div className="error-panel__title">无法读取当前状态</div>
      <div className="error-panel__message">{props.error}</div>
      <Button type="button" tone="primary" onClick={props.onRetry}>重试</Button>
    </Panel>
  );
}

function normalizeRecordedKeys(keys: string[]) {
  const unique = Array.from(new Set(keys));
  return unique.sort((left, right) => keySortRank(left) - keySortRank(right));
}

function keySortRank(key: string) {
  const rank = new Map([
    ['Ctrl', 0],
    ['Shift', 1],
    ['Alt', 2],
    ['Space', 3],
    ['Enter', 4],
    ['Tab', 5],
    ['Esc', 6],
    ['Backspace', 7],
    ['Delete', 8],
  ]);
  return rank.get(key) ?? 100 + key.charCodeAt(0);
}

function mapKeyboardEventToKey(event: KeyboardEvent): string | null {
  switch (event.key) {
    case 'Control':
      return 'Ctrl';
    case 'Shift':
      return 'Shift';
    case 'Alt':
      return 'Alt';
    case ' ':
      return 'Space';
    case 'Enter':
      return 'Enter';
    case 'Tab':
      return 'Tab';
    case 'Escape':
      return 'Esc';
    case 'Backspace':
      return 'Backspace';
    case 'Delete':
      return 'Delete';
    default:
      if (/^[a-z]$/i.test(event.key)) return event.key.toUpperCase();
      if (/^[0-9]$/.test(event.key)) return event.key;
      return null;
  }
}
