import { useEffect, useState, type ReactNode } from 'react';
import { AppWindow, CircleHelp, Eye, ExternalLink, Gauge, MessagesSquare, Power, Star } from 'lucide-react';
import foregroundIcon from '../assets/foreground.png';
import { Switch } from './ui';
import { setLaunchAtStartup, setLatestContactMoveOnly, setLatestContactMoveToleranceMs, setPreemptPreviousStroke, type AppBootstrapDto } from '../lib/tauri';
import type { AppUpdaterState } from '../hooks/use-app-updater';

type SettingsTab = 'general' | 'advanced' | 'about';

export function SettingsPage(props: {
  data: AppBootstrapDto;
  busyKey: string | null;
  onOpenGithub: () => void;
  onOpenReleases: () => void;
  onOpenIssues: () => void;
  onOpenDiscussions: () => void;
  initialTab?: SettingsTab;
  updater: AppUpdaterState;
  runAction: (key: string, action: () => Promise<unknown>) => Promise<void>;
  setShowLaunchAtStartupOnMainPage: (enabled: boolean) => Promise<unknown>;
}) {
  const [tab, setTab] = useState<SettingsTab>(props.initialTab ?? 'general');
  const [moveToleranceMs, setMoveToleranceMs] = useState(props.data.latestContactMoveToleranceMs);

  useEffect(() => setMoveToleranceMs(props.data.latestContactMoveToleranceMs), [props.data.latestContactMoveToleranceMs]);

  const saveMoveTolerance = () => {
    if (moveToleranceMs === props.data.latestContactMoveToleranceMs) return;
    void props.runAction('latest-contact-move-tolerance', () => setLatestContactMoveToleranceMs(moveToleranceMs));
  };

  return (
    <div className="settings-page">
      <div className="settings-tabs" role="tablist" aria-label="设置分类">
        <SettingsTabButton active={tab === 'general'} label="通用" onClick={() => setTab('general')} />
        <SettingsTabButton active={tab === 'advanced'} label="高级" onClick={() => setTab('advanced')} />
        <SettingsTabButton active={tab === 'about'} label="关于" onClick={() => setTab('about')} />
      </div>

      <div className="settings-tab-panel" role="tabpanel">
        {tab === 'general' ? (
          <section className="settings-section">
            <div className="settings-section__header">
              <AppWindow aria-hidden="true" />
              <h2>窗口行为</h2>
            </div>
            <div className="settings-toggle-list">
              <SettingsToggleRow
                icon={<Power aria-hidden="true" />}
                title="开机自启"
                description="随 Windows 登录自动运行并驻留系统托盘。"
                checked={props.data.launchAtStartup}
                disabled={props.busyKey === 'launch-at-startup'}
                onChange={(enabled) => void props.runAction('launch-at-startup', () => setLaunchAtStartup(enabled))}
              />
              <SettingsToggleRow
                icon={<Eye aria-hidden="true" />}
                title="在主页面显示开机自启"
                description="控制主页面顶部是否显示开机自启开关。"
                checked={props.data.showLaunchAtStartupOnMainPage}
                disabled={props.busyKey === 'show-launch-at-startup-on-main-page'}
                onChange={(enabled) => void props.runAction('show-launch-at-startup-on-main-page', () => props.setShowLaunchAtStartupOnMainPage(enabled))}
              />
            </div>
          </section>
        ) : null}

        {tab === 'advanced' ? (
          <div className="settings-advanced-stack">
            <section className="settings-policy-card">
              <header className="settings-policy-card__header">
                <div className="settings-policy-card__icon"><Gauge aria-hidden="true" /></div>
                <div>
                  <h2>降低延迟策略</h2>
                  <p>在高频连续输入时，按需减少积压或优先处理新笔迹。</p>
                </div>
              </header>
              <div className="settings-policy-card__body">
                <SettingsToggleRow
                  title="单笔最新优先"
                  description="同一笔迹积压时，只保留最新移动点，降低长笔迹延迟。"
                  consequence="后果：笔迹细节可能减少，快速转折可能被简化。"
                  checked={props.data.latestContactMoveOnly}
                  disabled={props.busyKey === 'latest-contact-move-only'}
                  onChange={(enabled) => void props.runAction('latest-contact-move-only', () => setLatestContactMoveOnly(enabled))}
                />
                <div className="settings-range-row">
                  <div className="settings-range-row__copy">
                    <span>积压保留窗口</span>
                    <strong>{moveToleranceMs} ms</strong>
                  </div>
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    value={moveToleranceMs}
                    disabled={!props.data.latestContactMoveOnly || props.busyKey === 'latest-contact-move-tolerance'}
                    aria-label="单笔最新优先积压保留窗口"
                    onChange={(event) => setMoveToleranceMs(Number(event.target.value))}
                    onPointerUp={saveMoveTolerance}
                    onKeyUp={saveMoveTolerance}
                  />
                  <p>允许窗口内的移动点继续排队；0 ms 表示直接由最新点顶替。</p>
                </div>
                <SettingsToggleRow
                  title="新笔抢占旧笔"
                  description="新笔落下时，终止上一笔的待处理输入并立即切换。"
                  consequence="后果：上一笔可能提前中断或缺少收尾。"
                  checked={props.data.preemptPreviousStroke}
                  disabled={props.busyKey === 'preempt-previous-stroke'}
                  onChange={(enabled) => void props.runAction('preempt-previous-stroke', () => setPreemptPreviousStroke(enabled))}
                />
              </div>
            </section>
          </div>
        ) : null}

        {tab === 'about' ? (
          <section className="settings-about">
            <header className="settings-about__header">
              <h2>关于</h2>
            </header>
            <div className="settings-about-card">
              <div className="settings-about__identity">
                <div className="settings-about__mark">
                  <img src={foregroundIcon} alt="AirSlate" />
                </div>
                <div className="settings-about__copy">
                  <h3>AirSlate PC Server</h3>
                  <div className="settings-about__meta">
                    <span className="settings-badge">版本 v{props.updater.currentVersion ?? '读取中'}</span>
                    <UpdateStatus updater={props.updater} />
                  </div>
                </div>
              </div>
              <div className="settings-about__actions">
                <button type="button" className="ui-button ui-button--ghost settings-about-action" onClick={props.onOpenGithub}>
                  <ExternalLink aria-hidden="true" />
                  GitHub
                </button>
                <UpdateAction distribution={props.data.distribution} onOpenReleases={props.onOpenReleases} updater={props.updater} />
              </div>
            </div>
            <AboutUpdateSection updater={props.updater} />
            <div className="settings-about-community">
              <div className="settings-about-community__item settings-about-community__item--review">
                <Star aria-hidden="true" />
                <span><strong>期待你的好评</strong><small>如果 AirSlate 对你有所帮助，欢迎在鸿蒙应用商店留下五星好评。非常感谢你的支持！</small></span>
              </div>
              <button type="button" className="settings-about-community__item" onClick={props.onOpenIssues}>
                <CircleHelp aria-hidden="true" />
                <span><strong>问题反馈</strong><small>遇到 Bug 或异常行为，请前往 GitHub Issues 反馈。</small></span>
                <ExternalLink aria-hidden="true" />
              </button>
              <button type="button" className="settings-about-community__item" onClick={props.onOpenDiscussions}>
                <MessagesSquare aria-hidden="true" />
                <span><strong>交流讨论</strong><small>交流想法、提出建议、关注开发动态、提出问题或查找解决方案。</small></span>
                <ExternalLink aria-hidden="true" />
              </button>
            </div>
          </section>
        ) : null}
      </div>
    </div>
  );
}

function AboutUpdateSection(props: {
  updater: AppUpdaterState;
}) {
  const { info, phase, progress } = props.updater;
  const hasUpdate = Boolean(info?.availableVersion && info.availableVersion !== info.currentVersion && phase !== 'up-to-date');

  if (!props.updater.supported || (!hasUpdate && phase !== 'installing')) return null;

  return (
    <section className="settings-about-update">
      {hasUpdate && info ? (
        <div className="settings-about-update__available">
          <div className="settings-about-update__title">发现新版本 v{info.availableVersion}</div>
          {info.notes ? <div className="settings-about-update__notes">{info.notes}</div> : <p>此版本包含功能改进与问题修复。</p>}
          {phase === 'installing' && progress !== null ? (
            <div className="settings-about-update__progress" aria-label={`更新进度 ${Math.round(progress * 100)}%`}>
              <span style={{ width: `${Math.round(progress * 100)}%` }} />
            </div>
          ) : null}
        </div>
      ) : null}

    </section>
  );
}

function UpdateStatus(props: { updater: AppUpdaterState }) {
  const { updater } = props;
  if (!updater.supported) return <span>当前平台暂不支持自动更新</span>;
  if (updater.error) return <span className="settings-update-status--error">网络错误，请重试</span>;
  if (updater.phase === 'checking') return <span>正在检查更新…</span>;
  if (updater.phase === 'installing') return <span>正在安装更新…</span>;
  if (updater.phase === 'up-to-date') return <span className="settings-update-status--success">当前已是最新版本</span>;
  if (updater.info?.availableVersion && updater.info.availableVersion !== updater.info.currentVersion) {
    return <span className="settings-update-status--available">发现新版本 v{updater.info.availableVersion}</span>;
  }
  return <span>尚未检查更新</span>;
}

function UpdateAction(props: { distribution: AppBootstrapDto['distribution']; onOpenReleases: () => void; updater: AppUpdaterState }) {
  const { info, phase } = props.updater;
  if (!props.updater.supported) return null;
  const hasUpdate = Boolean(info?.availableVersion && info.availableVersion !== info.currentVersion && phase !== 'up-to-date');
  if (hasUpdate) {
    if (props.distribution === 'portable') {
      return <button type="button" className="ui-button ui-button--primary settings-about-action" onClick={props.onOpenReleases}>前往下载</button>;
    }
    return <button type="button" className="ui-button ui-button--primary settings-about-action" onClick={() => void props.updater.installUpdate()} disabled={phase === 'installing'}>{phase === 'installing' ? '正在更新…' : `更新到 v${info?.availableVersion ?? ''}`}</button>;
  }
  return <button type="button" className="ui-button ui-button--ghost settings-about-action" onClick={() => void props.updater.checkForUpdates()} disabled={phase === 'checking'}>{phase === 'checking' ? '正在检查…' : '检查更新'}</button>;
}

function SettingsToggleRow(props: {
  icon?: ReactNode;
  title: string;
  description: string;
  consequence?: string;
  checked: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}) {
  return (
    <div className="settings-toggle-row">
      {props.icon ? <div className="settings-toggle-row__icon">{props.icon}</div> : null}
      <div className="settings-toggle-row__copy">
        <h3 className="settings-toggle-row__title">{props.title}</h3>
        <p>{props.description}</p>
        {props.consequence ? <p className="settings-toggle-row__consequence">{props.consequence}</p> : null}
      </div>
      <Switch checked={props.checked} disabled={props.disabled} ariaLabel={props.title} onChange={props.onChange} />
    </div>
  );
}

function SettingsTabButton(props: { active: boolean; label: string; onClick: () => void }) {
  return (
    <button type="button" role="tab" aria-selected={props.active} className={props.active ? 'settings-tab settings-tab--active' : 'settings-tab'} onClick={props.onClick}>
      {props.label}
    </button>
  );
}
