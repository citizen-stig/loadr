// Reusable, brand-styled form & layout primitives. Centralising the visual
// language here keeps every screen consistent and the feature code declarative
// (`<Field label>… <TextInput/>`) instead of a wall of repeated Tailwind classes.

import { cloneElement, isValidElement, useId, useState } from 'react';
import type {
  ButtonHTMLAttributes, InputHTMLAttributes, ReactElement, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes,
} from 'react';

import { ChevronDown, ChevronRight, type Icon } from './icons';

// ---- Button ---------------------------------------------------------------
type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
type Size = 'sm' | 'md';

const BTN_BASE =
  'inline-flex items-center justify-center gap-1.5 rounded-lg font-semibold whitespace-nowrap ' +
  'transition-colors duration-150 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-40';

const BTN_VARIANT: Record<Variant, string> = {
  primary:
    'bg-gradient-to-b from-ember to-blood text-white shadow-[0_1px_0_0_rgb(255_255_255/0.12)_inset,0_8px_24px_-12px_rgb(220_38_38/0.7)] ' +
    'hover:from-flare hover:to-ember active:translate-y-px',
  secondary:
    'border border-edge bg-panel text-ash hover:border-edge-bright hover:bg-panel-2 hover:text-white',
  ghost: 'text-smoke hover:bg-edge/60 hover:text-ash',
  danger: 'border border-blood/40 bg-blood/10 text-flare hover:border-blood hover:bg-blood/20',
};

const BTN_SIZE: Record<Size, string> = {
  sm: 'px-2.5 py-1 text-xs',
  md: 'px-3.5 py-1.5 text-sm',
};

export function Button({
  variant = 'secondary', size = 'md', icon: IconC, children, className = '', ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: Size; icon?: Icon }) {
  return (
    <button className={`${BTN_BASE} ${BTN_VARIANT[variant]} ${BTN_SIZE[size]} ${className}`} {...props}>
      {IconC && <IconC className={size === 'sm' ? 'text-[0.95em]' : 'text-base'} />}
      {children}
    </button>
  );
}

export function IconButton({
  icon: IconC, label, tone = 'neutral', className = '', ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { icon: Icon; label: string; tone?: 'neutral' | 'danger' }) {
  const toneCls =
    tone === 'danger'
      ? 'text-mist hover:bg-blood/15 hover:text-flare'
      : 'text-mist hover:bg-edge/70 hover:text-ash';
  return (
    <button
      aria-label={label}
      title={label}
      className={`inline-flex h-7 w-7 items-center justify-center rounded-md text-[15px] transition-colors disabled:opacity-30 disabled:hover:bg-transparent ${toneCls} ${className}`}
      {...props}
    >
      <IconC />
    </button>
  );
}

// ---- Field + inputs -------------------------------------------------------
const CONTROL =
  'w-full rounded-lg border border-edge bg-coal px-2.5 py-1.5 text-sm text-ash placeholder:text-mist ' +
  'transition-colors focus:border-ember/70 focus:outline-none focus:ring-2 focus:ring-ember/25';

// Associates the visible caption with its control programmatically: an id is
// generated and injected onto the single child element so screen readers and
// `getByLabelText` resolve the control. The hint sits outside the label so it
// doesn't pollute the control's accessible name.
export function Field({
  label, hint, children, className = '',
}: { label?: ReactNode; hint?: ReactNode; children: ReactNode; className?: string }) {
  const id = useId();
  const control =
    label && isValidElement(children)
      ? cloneElement(children as ReactElement<{ id?: string }>, {
          id: (children as ReactElement<{ id?: string }>).props.id ?? id,
        })
      : children;
  return (
    <div className={`flex flex-col gap-1 ${className}`}>
      {label && (
        <label htmlFor={id} className="text-[11px] font-semibold uppercase tracking-wide text-smoke">
          {label}
        </label>
      )}
      {control}
      {hint && <span className="text-[11px] text-mist">{hint}</span>}
    </div>
  );
}

export function TextInput({ className = '', ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input type="text" className={`${CONTROL} ${className}`} {...props} />;
}

export function NumberInput({ className = '', ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input type="number" className={`${CONTROL} ${className}`} {...props} />;
}

export function Textarea({ className = '', ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      className={`${CONTROL} resize-y font-mono text-[13px] leading-relaxed ${className}`}
      spellCheck={false}
      {...props}
    />
  );
}

export function Select({ className = '', children, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={`${CONTROL} cursor-pointer appearance-none bg-[length:0] pr-2 ${className}`} {...props}>
      {children}
    </select>
  );
}

export function Checkbox({ label, checked, onChange }: { label: ReactNode; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="inline-flex cursor-pointer items-center gap-2 text-xs text-smoke">
      <input type="checkbox" className="h-3.5 w-3.5 accent-ember" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {label}
    </label>
  );
}

// Collapsible section (native disclosure pattern) for advanced/optional fields.
export function Disclosure({ label, children, defaultOpen = false }: { label: ReactNode; children: ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="rounded-lg border border-edge">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center gap-1.5 px-3 py-2 text-xs font-semibold uppercase tracking-wide text-smoke transition-colors hover:text-ash"
      >
        <span className="text-mist">{open ? <ChevronDown /> : <ChevronRight />}</span>
        {label}
      </button>
      {open && <div className="space-y-3 border-t border-edge p-3">{children}</div>}
    </div>
  );
}

// ---- Surfaces -------------------------------------------------------------
export function Card({ className = '', children, ...rest }: { className?: string; children: ReactNode } & Record<string, unknown>) {
  return (
    <div className={`rounded-xl border border-edge bg-panel ${className}`} {...rest}>
      {children}
    </div>
  );
}

export function Badge({
  children, tone = 'neutral', className = '',
}: { children: ReactNode; tone?: 'neutral' | 'ember' | 'ok' | 'warn'; className?: string }) {
  const tones = {
    neutral: 'border-edge bg-edge/40 text-smoke',
    ember: 'border-ember/40 bg-ember/10 text-flare',
    ok: 'border-ok/40 bg-ok/10 text-ok',
    warn: 'border-warn/40 bg-warn/10 text-warn',
  } as const;
  return (
    <span className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide ${tones[tone]} ${className}`}>
      {children}
    </span>
  );
}

// ---- Segmented control (view toggles) -------------------------------------
export function Segmented<T extends string>({
  value, onChange, options, ariaLabel,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string; icon?: Icon }[];
  ariaLabel: string;
}) {
  return (
    <div role="tablist" aria-label={ariaLabel} className="inline-flex rounded-lg border border-edge bg-coal p-0.5">
      {options.map((o) => {
        const active = o.value === value;
        const IconC = o.icon;
        return (
          <button
            key={o.value}
            role="tab"
            aria-selected={active}
            onClick={() => onChange(o.value)}
            className={`inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-semibold transition-colors ${
              active ? 'bg-panel-2 text-white shadow-sm' : 'text-smoke hover:text-ash'
            }`}
          >
            {IconC && <IconC className="text-sm" />}
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
