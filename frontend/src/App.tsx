import { useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ArrowUpCircle, Clock3, Keyboard, Monitor, Power, Settings, Unplug } from 'lucide-react';
import { ConnectionPage } from './components/connection-page';
import { SettingsPage } from './components/settings-page';
import { Shell } from './components/shell';
import { ShortcutsPage, type RecordingTarget } from './components/shortcuts-page';
import { Button, EmptyState, Panel, Switch } from './components/ui';
import { useAppUpdater } from './hooks/use-app-updater';
import {
  type AppBootstrapDto,
  type SessionStatusEvent,
  type UsbStatusEvent,
  disconnectActiveSession,
  getAppBootstrap,
  getLanIpv4Values,
  openExternal,
  retryUsbConnection,
  setBindingKeys,
  setLaunchAtStartup,
  setRadialOuterSlot,
  setShowLaunchAtStartupOnMainPage,
  setWiredConnectionEnabled,
} from './lib/tauri';

type PageKey = 'connection' | 'shortcuts' | 'settings';
type SettingsTab = 'general' | 'advanced' | 'about';
const GITHUB_URL = 'https://github.com/2doright/airslate-pc-server';
const RELEASES_URL = `${GITHUB_URL}/releases`;
const ISSUES_URL = `${GITHUB_URL}/issues`;
const DISCUSSIONS_URL = `${GITHUB_URL}/discussions`;

export function App() {
  const [page, setPage] = useState<PageKey>('connection');
  const [settingsReturnPage, setSettingsReturnPage] = useState<'connection' | 'shortcuts'>('connection');
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>('general');
  const [data, setData] = useState<AppBootstrapDto | null>(null);
  const [hasActiveSession, setHasActiveSession] = useState(false);
  const [usbStatus, setUsbStatus] = useState<UsbStatusEvent>({
    state: 'waiting',
    detail: '等待 AirSlate 平板 USB 连接',
    retryable: true,
    device: null,
  });
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshingIpv4, setRefreshingIpv4] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [newPresetName, setNewPresetName] = useState('');
  const [recordingTarget, setRecordingTarget] = useState<RecordingTarget | null>(null);
  const usbStatusRevision = useRef(0);
  const updater = useAppUpdater(data?.distribution, /Windows/i.test(navigator.userAgent));

  const applySessionStatus = (nextStatus: boolean) => {
    setHasActiveSession(nextStatus);
    setData((current) => current ? {
      ...current,
      sessionStatus: { ...current.sessionStatus, hasActiveSession: nextStatus },
    } : current);
  };

  const load = async (options?: { preserveView?: boolean }) => {
    const preserveView = options?.preserveView ?? false;
    const usbStatusRevisionAtStart = usbStatusRevision.current;
    if (preserveView) {
      setRefreshing(true);
    } else {
      setLoading(true);
      setError(null);
    }
    try {
      const next = await getAppBootstrap();
      setData(next);
      setHasActiveSession(next.sessionStatus.hasActiveSession);
      if (usbStatusRevision.current === usbStatusRevisionAtStart) {
        setUsbStatus(next.usbStatus);
      }
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

  const openSettings = () => {
    setSettingsInitialTab('general');
    if (page !== 'settings') setSettingsReturnPage(page);
    setPage('settings');
  };

  const openAbout = () => {
    setSettingsInitialTab('about');
    if (page !== 'settings') setSettingsReturnPage(page);
    setPage('settings');
  };

  const handleOpenExternal = (url: string) => {
    void openExternal(url).catch((err) => {
      setError(err instanceof Error ? err.message : String(err));
    });
  };

  const handleDisconnect = async () => {
    setDisconnecting(true);
    try {
      const status = await disconnectActiveSession();
      applySessionStatus(status.hasActiveSession);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setDisconnecting(false);
    }
  };

  const handleRefreshIpv4 = async () => {
    if (refreshingIpv4) return;
    setRefreshingIpv4(true);
    try {
      const ipv4Values = await getLanIpv4Values();
      setData((current) => (current ? { ...current, ipv4Values } : current));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRefreshingIpv4(false);
    }
  };

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen<SessionStatusEvent>('session-status-changed', (event) => {
      applySessionStatus(event.payload.hasActiveSession);
    })
      .then((removeListener) => {
        if (disposed) {
          removeListener();
        } else {
          unlisten = removeListener;
        }
      })
      .catch((err) => {
        if (!disposed) {
          setError(err instanceof Error ? err.message : String(err));
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<UsbStatusEvent>('usb-status-changed', (event) => {
      usbStatusRevision.current += 1;
      setUsbStatus(event.payload);
    })
      .then((removeListener) => disposed ? removeListener() : (unlisten = removeListener))
      .catch((err) => { if (!disposed) setError(err instanceof Error ? err.message : String(err)); });
    return () => { disposed = true; unlisten?.(); };
  }, []);

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
      const mapped = mapKeyboardEventToKey(event);
      if (mapped) pressed.add(mapped);
    };

    const handleKeyUp = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
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
      { key: 'connection', label: '连接', icon: <Monitor />, active: page === 'connection', onClick: () => setPage('connection') },
      { key: 'shortcuts', label: '快捷键', icon: <Keyboard />, active: page === 'shortcuts', onClick: () => setPage('shortcuts') },
    ],
    [page],
  );

  const selectedMonitor = data?.monitors.find((monitor) => monitor.selected) ?? data?.monitors[0] ?? null;
  const hasAvailableUpdate = Boolean(
    updater.info &&
    updater.info.availableVersion !== updater.info.currentVersion &&
    updater.phase !== 'up-to-date' &&
    updater.phase !== 'idle',
  );

  return (
    <Shell
      appName="AirSlate"
      appUrl={GITHUB_URL}
      onAppUrlClick={() => handleOpenExternal(GITHUB_URL)}
      appNameAccessory={
        hasAvailableUpdate ? (
          <button type="button" className="shell-update-indicator" onClick={openAbout} aria-label="发现新版本，打开关于" title="发现新版本">
            <ArrowUpCircle className="shell-update-indicator__icon" />
            <span>有新版本</span>
          </button>
        ) : null
      }
      nav={nav}
      subpage={page === 'settings' ? { title: '设置', onBack: () => setPage(settingsReturnPage) } : undefined}
      navAccessory={
        <button
          type="button"
          className={page === 'settings' ? 'shell-utility-button shell-utility-button--active' : 'shell-utility-button'}
          onClick={openSettings}
          aria-label="设置"
          title="设置"
        >
          <Settings className="shell-lucide-icon shell-lucide-icon--small" />
        </button>
      }
      headerAccessory={
        data?.showLaunchAtStartupOnMainPage ? (
          <div className="shell-startup-toggle" title="开机自启">
            <Power className={data.launchAtStartup ? 'shell-startup-toggle__icon shell-startup-toggle__icon--enabled' : 'shell-startup-toggle__icon'} aria-hidden="true" />
            <Switch
              checked={data.launchAtStartup}
              disabled={loading || refreshing || busyKey === 'launch-at-startup'}
              ariaLabel="切换开机自启"
              onChange={(enabled) => void runAction('launch-at-startup', () => setLaunchAtStartup(enabled))}
            />
          </div>
        ) : null
      }
      meta={null}
      actions={
        page === 'settings' ? null : (
          <Button
            type="button"
            tone={hasActiveSession ? 'danger' : 'ghost'}
            className={hasActiveSession ? 'shell-session-button shell-session-button--connected' : 'shell-session-button shell-session-button--awaiting'}
            onClick={() => void handleDisconnect()}
            disabled={!data || !hasActiveSession || disconnecting}
            aria-label={hasActiveSession ? '断开现有/残留连接' : '等待连接'}
            title={hasActiveSession ? `断开现有/残留连接（${usbStatus.detail}）` : usbStatus.detail}
          >
            {hasActiveSession ? <Unplug className="shell-lucide-icon" /> : <Clock3 className="shell-lucide-icon" />}
          </Button>
        )
      }
    >
      {loading && <LoadingState />}
      {!loading && error && !data && <ErrorState error={error} onRetry={() => void load()} />}
      {!loading && data && (
        <div className="page-stack">
          {error ? <ErrorState error={error} onRetry={() => void load({ preserveView: true })} /> : null}
          {page === 'connection' ? (
            <ConnectionPage
              data={data}
              usbStatus={usbStatus}
              selectedMonitor={selectedMonitor}
              busyKey={busyKey}
              runAction={runAction}
              refreshingIpv4={refreshingIpv4}
              onRefreshIpv4={() => void handleRefreshIpv4()}
              onRetryUsb={() => void runAction('usb-retry', retryUsbConnection)}
              onSetWiredConnectionEnabled={(enabled) => void runAction(
                'wired-connection',
                () => setWiredConnectionEnabled(enabled),
              )}
            />
          ) : page === 'shortcuts' ? (
            <ShortcutsPage
              data={data}
              busyKey={busyKey}
              newPresetName={newPresetName}
              setNewPresetName={setNewPresetName}
              runAction={runAction}
              recordingTarget={recordingTarget}
              setRecordingTarget={setRecordingTarget}
            />
          ) : (
            <SettingsPage
              data={data}
              busyKey={busyKey}
              onOpenGithub={() => handleOpenExternal(GITHUB_URL)}
              onOpenReleases={() => handleOpenExternal(RELEASES_URL)}
              onOpenIssues={() => handleOpenExternal(ISSUES_URL)}
              onOpenDiscussions={() => handleOpenExternal(DISCUSSIONS_URL)}
              initialTab={settingsInitialTab}
              updater={updater}
              runAction={runAction}
              setShowLaunchAtStartupOnMainPage={setShowLaunchAtStartupOnMainPage}
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
    ['左 Ctrl', 1], ['右 Ctrl', 2],
    ['Shift', 3], ['左 Shift', 4], ['右 Shift', 5],
    ['Alt', 6], ['左 Alt', 7], ['右 Alt', 8],
    ['左 Win', 9], ['右 Win', 10],
  ]);
  return rank.get(key) ?? 100 + key.charCodeAt(0);
}

function mapKeyboardEventToKey(event: KeyboardEvent): string | null {
  const namedCodes: Record<string, string> = {
    ControlLeft: '左 Ctrl', ControlRight: '右 Ctrl', ShiftLeft: '左 Shift', ShiftRight: '右 Shift',
    AltLeft: '左 Alt', AltRight: '右 Alt', MetaLeft: '左 Win', MetaRight: '右 Win',
    Space: 'Space', Enter: 'Enter', NumpadEnter: 'Num Enter', Tab: 'Tab', Escape: 'Esc',
    Backspace: 'Backspace', Delete: 'Delete', Insert: 'Insert', Home: 'Home', End: 'End',
    PageUp: 'Page Up', PageDown: 'Page Down', ArrowUp: '↑', ArrowDown: '↓',
    ArrowLeft: '←', ArrowRight: '→', CapsLock: 'Caps Lock', NumLock: 'Num Lock',
    ScrollLock: 'Scroll Lock', PrintScreen: 'Print Screen', Pause: 'Pause', ContextMenu: '菜单键',
    Backquote: '`', Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']',
    Backslash: '\\', Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Slash: '/',
    NumpadAdd: 'Num +', NumpadSubtract: 'Num -', NumpadMultiply: 'Num *',
    NumpadDivide: 'Num /', NumpadDecimal: 'Num .',
    AudioVolumeMute: '静音', AudioVolumeDown: '音量 -', AudioVolumeUp: '音量 +',
    MediaTrackPrevious: '上一曲', MediaTrackNext: '下一曲', MediaPlayPause: '播放/暂停',
    MediaStop: '停止', BrowserBack: '浏览器后退', BrowserForward: '浏览器前进',
    BrowserRefresh: '浏览器刷新', BrowserStop: '浏览器停止', BrowserSearch: '浏览器搜索',
    BrowserFavorites: '浏览器收藏', BrowserHome: '浏览器主页',
  };
  const named = namedCodes[event.code];
  if (named) return named;

  const letter = event.code.match(/^Key([A-Z])$/);
  if (letter) return letter[1];
  const digit = event.code.match(/^Digit([0-9])$/);
  if (digit) return digit[1];
  const numpad = event.code.match(/^Numpad([0-9])$/);
  if (numpad) return `Num ${numpad[1]}`;
  const functionKey = event.code.match(/^F([1-9]|1[0-9]|2[0-4])$/);
  if (functionKey) return `F${functionKey[1]}`;
  return null;
}
