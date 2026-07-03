import { useEffect, useMemo, useRef, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from 'react';
import { cn } from '../lib/cn';

export function Panel(props: { children: ReactNode; className?: string; variant?: 'default' | 'hero' | 'tight' }) {
  return <section className={cn('ui-panel', props.variant === 'hero' && 'ui-panel--hero', props.variant === 'tight' && 'ui-panel--tight', props.className)}>{props.children}</section>;
}

export function PanelHeader(props: { eyebrow?: string; title: ReactNode; action?: ReactNode; children?: ReactNode }) {
  return (
    <div className="panel-header">
      <div className="min-w-0">
        {props.eyebrow ? <div className="panel-eyebrow">{props.eyebrow}</div> : null}
        <h2 className="panel-title">{props.title}</h2>
        {props.children ? <div className="panel-copy">{props.children}</div> : null}
      </div>
      {props.action ? <div className="panel-action">{props.action}</div> : null}
    </div>
  );
}

export function Button(props: ButtonHTMLAttributes<HTMLButtonElement> & { tone?: 'primary' | 'ghost' | 'danger'; wide?: boolean }) {
  const { tone, wide, className, ...rest } = props;
  return <button className={cn('ui-button', tone && `ui-button--${tone}`, wide && 'ui-button--wide', className)} {...rest} />;
}

export function Switch(props: {
  checked: boolean;
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.checked}
      aria-label={props.ariaLabel}
      disabled={props.disabled}
      className={cn('ui-switch', props.checked && 'ui-switch--checked', props.className)}
      onClick={() => props.onChange(!props.checked)}
    >
      <span className="ui-switch__track">
        <span className="ui-switch__thumb" />
      </span>
    </button>
  );
}

export function TextInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={cn('ui-input', props.className)} />;
}

export function SelectField(props: {
  value: string;
  options: Array<{
    value: string;
    label: string;
    action?: {
      ariaLabel: string;
      disabled?: boolean;
      icon: ReactNode;
      onClick: () => void;
    };
  }>;
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const selectedOption = useMemo(() => props.options.find((option) => option.value === props.value), [props.options, props.value]);

  useEffect(() => {
    if (!open) return;
    const closeOnOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    window.addEventListener('pointerdown', closeOnOutside);
    window.addEventListener('keydown', closeOnEscape);
    return () => {
      window.removeEventListener('pointerdown', closeOnOutside);
      window.removeEventListener('keydown', closeOnEscape);
    };
  }, [open]);

  return (
    <div ref={rootRef} className={cn('ui-select-shell', open && 'ui-select-shell--open', props.className)}>
      <button
        type="button"
        className="ui-select"
        disabled={props.disabled}
        aria-label={props.ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span>{selectedOption?.label ?? '请选择'}</span>
      </button>
      {open ? (
        <div className="ui-select-menu" role="listbox" aria-label={props.ariaLabel}>
          {props.options.map((option) => {
            const selected = option.value === props.value;
            const action = option.action;
            return (
              <div key={option.value} className="ui-select-option-row">
                <button
                  type="button"
                  role="option"
                  aria-selected={selected}
                  className={cn('ui-select-option', selected && 'ui-select-option--selected')}
                  onClick={() => {
                    props.onChange(option.value);
                    setOpen(false);
                  }}
                >
                  <span className="ui-select-option__label">{option.label}</span>
                </button>
                {action ? (
                  <button
                    type="button"
                    className="ui-select-option-action"
                    aria-label={action.ariaLabel}
                    title={action.ariaLabel}
                    disabled={action.disabled}
                    onClick={(event) => {
                      event.stopPropagation();
                      setOpen(false);
                      action.onClick();
                    }}
                  >
                    {action.icon}
                  </button>
                ) : null}
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

export function Badge(props: { children: ReactNode; tone?: 'neutral' | 'success' | 'accent' | 'warning' }) {
  return <span className={cn('ui-badge', props.tone && `ui-badge--${props.tone}`)}>{props.children}</span>;
}

export function KeyToken(props: { children: ReactNode; soft?: boolean }) {
  return <span className={cn('key-token', props.soft && 'key-token--soft')}>{props.children}</span>;
}

export function EmptyState(props: { title: string; children?: ReactNode }) {
  return (
    <div className="empty-state">
      <div className="empty-state__title">{props.title}</div>
      {props.children ? <div className="empty-state__copy">{props.children}</div> : null}
    </div>
  );
}
