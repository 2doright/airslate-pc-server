import type { ReactNode } from 'react';
import { cn } from '../lib/cn';

export function SectionCard(props: {
  title?: string;
  subtitle?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn('panel-card rounded-[20px] p-4', props.className)}>
      {props.title || props.subtitle ? (
        <div className="mb-3">
          {props.title ? <h2 className="text-[15px] font-semibold text-slate-950">{props.title}</h2> : null}
          {props.subtitle ? <p className="mt-1 text-sm leading-6 text-slate-500">{props.subtitle}</p> : null}
        </div>
      ) : null}
      {props.children}
    </section>
  );
}

export function Pill(props: { children: React.ReactNode; tone?: 'neutral' | 'success' | 'accent' | 'warning' }) {
  const tone =
    props.tone === 'success'
      ? 'border-emerald-200 bg-emerald-50 text-emerald-700'
      : props.tone === 'accent'
        ? 'border-sky-200 bg-sky-50 text-sky-700'
        : props.tone === 'warning'
          ? 'border-amber-200 bg-amber-50 text-amber-700'
          : 'border-slate-200 bg-slate-50 text-slate-600';

  return <span className={cn('inline-flex rounded-full border px-2.5 py-0.5 text-[11px] font-semibold', tone)}>{props.children}</span>;
}
