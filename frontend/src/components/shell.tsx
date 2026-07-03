import type { ReactNode } from 'react';
import foregroundIcon from '../assets/foreground.png';
import { cn } from '../lib/cn';

export function Shell(props: {
  appName: string;
  nav: Array<{ key: string; label: string; icon: ReactNode; active: boolean; onClick: () => void }>;
  children: ReactNode;
  meta: ReactNode;
  navAccessory?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="app-shell">
      <div className="shell-frame">
        <header className="shell-header">
          <div className="brand-card">
            <div className="brand-mark">
              <img src={foregroundIcon} alt="AirSlate" />
            </div>
            <h1>{props.appName}</h1>
          </div>

          <div className="shell-nav-group">
            <nav className="shell-nav" aria-label="主导航">
              {props.nav.map((item) => (
                <button key={item.key} type="button" onClick={item.onClick} className={cn('shell-nav__item', item.active && 'shell-nav__item--active')}>
                  <span className="shell-nav__icon">{item.icon}</span>
                  <span>{item.label}</span>
                </button>
              ))}
            </nav>
            {props.navAccessory ? <div className="shell-nav-accessory">{props.navAccessory}</div> : null}
          </div>

          <div className="shell-topbar">
            <div className="shell-status">{props.meta}</div>
            <div className="shell-actions">{props.actions}</div>
          </div>
        </header>

        <main className="shell-content">{props.children}</main>
      </div>
    </div>
  );
}
