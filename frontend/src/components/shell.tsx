import type { ReactNode } from 'react';
import { ArrowLeft } from 'lucide-react';
import { cn } from '../lib/cn';

export function Shell(props: {
  appName: string;
  appUrl?: string;
  onAppUrlClick?: () => void;
  appNameAccessory?: ReactNode;
  nav: Array<{ key: string; label: string; icon: ReactNode; active: boolean; onClick: () => void }>;
  subpage?: { title: string; onBack: () => void };
  children: ReactNode;
  meta: ReactNode;
  navAccessory?: ReactNode;
  headerAccessory?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="app-shell">
      <div className="shell-frame">
        <header className="shell-header">
          <div className="shell-header__start">
            {props.subpage ? (
              <div className="shell-subpage-heading">
                <button type="button" className="shell-back-button" onClick={props.subpage.onBack} aria-label="返回" title="返回">
                  <ArrowLeft className="shell-lucide-icon shell-lucide-icon--small" />
                </button>
                <h1 className="shell-subpage-title">{props.subpage.title}</h1>
              </div>
            ) : (
              <>
                <div className="shell-brand-group">
                  {props.onAppUrlClick ? (
                    <button type="button" className="brand-card" onClick={props.onAppUrlClick} aria-label={`${props.appName} GitHub 页面`}>
                      <h1>{props.appName}</h1>
                    </button>
                  ) : (
                    <a className="brand-card" href={props.appUrl} target="_blank" rel="noreferrer" aria-label={`${props.appName} GitHub 页面`}>
                      <h1>{props.appName}</h1>
                    </a>
                  )}
                  {props.appNameAccessory ? <div className="shell-app-name-accessory">{props.appNameAccessory}</div> : null}
                </div>
                {props.navAccessory ? <div className="shell-nav-accessory">{props.navAccessory}</div> : null}
              </>
            )}
          </div>

          <div className="shell-header__end">
            {!props.subpage && props.headerAccessory ? <div className="shell-header-accessory">{props.headerAccessory}</div> : null}
            {!props.subpage ? (
              <nav className="shell-nav" aria-label="主导航">
                {props.nav.map((item) => (
                  <button key={item.key} type="button" onClick={item.onClick} className={cn('shell-nav__item', item.active && 'shell-nav__item--active')} aria-label={item.label} title={item.label}>
                    <span className="shell-nav__icon">{item.icon}</span>
                    <span className="shell-nav__label">{item.label}</span>
                  </button>
                ))}
              </nav>
            ) : null}
            <div className="shell-tool-group">
              <div className="shell-status">{props.meta}</div>
              <div className="shell-actions">{props.actions}</div>
            </div>
          </div>
        </header>

        <main className="shell-content">{props.children}</main>
      </div>
    </div>
  );
}
