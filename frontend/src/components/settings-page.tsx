import { useState, type ReactNode } from 'react';
import { AppWindow, Eye, ExternalLink, Info, Power } from 'lucide-react';
import foregroundIcon from '../assets/foreground.png';
import { Switch } from './ui';
import { setLaunchAtStartup, type AppBootstrapDto } from '../lib/tauri';
import type { AppUpdaterState } from '../hooks/use-app-updater';

type SettingsTab = 'general' | 'advanced' | 'about';

export function SettingsPage(props: {
  data: AppBootstrapDto;
  busyKey: string | null;
  onOpenGithub: () => void;
  onOpenReleases: () => void;
  initialTab?: SettingsTab;
  updater: AppUpdaterState;
  runAction: (key: string, action: () => Promise<unknown>) => Promise<void>;
  setShowLaunchAtStartupOnMainPage: (enabled: boolean) => Promise<unknown>;
}) {
  const [tab, setTab] = useState<SettingsTab>(props.initialTab ?? 'general');

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
          <div className="settings-note">
            <div className="settings-note__title">高级配置</div>
            <p>压感曲线、手势与按键映射均会在对应功能页中直接应用，无需额外保存。</p>
          </div>
        ) : null}

        {tab === 'about' ? (
          <section className="settings-about">
            <header className="settings-about__header">
              <h2>关于</h2>
              <p>关于 AirSlate PC Server 的版本与项目信息。</p>
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
                    <span>Windows 手写笔与触控手势服务</span>
                  </div>
                </div>
              </div>
              <button type="button" className="ui-button ui-button--ghost settings-about-action" onClick={props.onOpenGithub}>
                <ExternalLink aria-hidden="true" />
                GitHub
              </button>
            </div>
            <AboutUpdateSection
              distribution={props.data.distribution}
              onOpenReleases={props.onOpenReleases}
              updater={props.updater}
            />
            <div className="settings-about-note">
              <Info aria-hidden="true" />
              <p>AirSlate PC Server 将局域网手写笔输入转换为 Windows 笔事件，并提供压感、显示器映射与快捷键配置。</p>
            </div>
          </section>
        ) : null}
      </div>
    </div>
  );
}

function AboutUpdateSection(props: {
  distribution: AppBootstrapDto['distribution'];
  onOpenReleases: () => void;
  updater: AppUpdaterState;
}) {
  const { info, phase, error, progress } = props.updater;
  const currentVersion = info?.currentVersion ?? '读取中';
  const isPortable = props.distribution === 'portable';
  const isBusy = phase === 'checking' || phase === 'installing';
  const hasUpdate = Boolean(info?.availableVersion && info.availableVersion !== info.currentVersion && phase !== 'up-to-date');

  if (!props.updater.supported) {
    return (
      <section className="settings-about-update">
        <div className="settings-about-update__header">
          <div>
            <h3>版本更新</h3>
            <p>当前平台暂不在 Windows x64 自动更新范围内。</p>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="settings-about-update">
      <div className="settings-about-update__header">
        <div>
          <h3>版本更新</h3>
          <p>当前版本 v{currentVersion}</p>
        </div>
        <div className="settings-about-update__actions">
          {hasUpdate ? (
            isPortable ? (
              <button type="button" className="ui-button ui-button--primary settings-about-action" onClick={props.onOpenReleases}>
                前往下载
              </button>
            ) : (
              <button type="button" className="ui-button ui-button--primary settings-about-action" onClick={() => void props.updater.installUpdate()} disabled={isBusy}>
                {phase === 'installing' ? '正在更新…' : `更新到 v${info?.availableVersion ?? ''}`}
              </button>
            )
          ) : (
            <button type="button" className="ui-button ui-button--ghost settings-about-action" onClick={() => void props.updater.checkForUpdates()} disabled={isBusy}>
              {phase === 'checking' ? '正在检查…' : '检查更新'}
            </button>
          )}
        </div>
      </div>

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

      {phase === 'up-to-date' ? <p className="settings-about-update__status settings-about-update__status--success">当前已是最新版本。</p> : null}
      {error ? (
        <div className="settings-about-update__error">
          <p>更新失败：{error}</p>
          <div className="settings-about-update__error-actions">
            <button
              type="button"
              className="ui-button ui-button--ghost settings-about-action"
              onClick={() => void (hasUpdate && !isPortable ? props.updater.installUpdate() : props.updater.checkForUpdates())}
              disabled={isBusy}
            >
              重试
            </button>
            <button type="button" className="ui-button ui-button--ghost settings-about-action" onClick={props.onOpenReleases}>打开下载页</button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function SettingsToggleRow(props: {
  icon: ReactNode;
  title: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onChange: (enabled: boolean) => void;
}) {
  return (
    <div className="settings-toggle-row">
      <div className="settings-toggle-row__icon">{props.icon}</div>
      <div className="settings-toggle-row__copy">
        <div className="settings-toggle-row__title">{props.title}</div>
        <p>{props.description}</p>
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
