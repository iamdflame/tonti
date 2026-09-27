import type { ReactNode } from 'react';
import { CircleAlert, Hourglass } from 'lucide-react';

/** A status line on light surfaces: an icon and words, never a colour on its own (DESIGN.md).
 * `wait` is for things not open yet; `hold` for something the member must act on. */
export function Notice({ children, kind = 'wait', className }: { children: ReactNode; kind?: 'wait' | 'hold'; className?: string }) {
  const Icon = kind === 'hold' ? CircleAlert : Hourglass;
  return (
    <div role="status" className={`flex items-start gap-3 rounded-2xl bg-paper p-4 text-body shadow-sm ring-1 ${kind === 'hold' ? 'ring-danger/40' : 'ring-ink/10'} ${className ?? ''}`}>
      <Icon className={`mt-0.5 size-5 shrink-0 ${kind === 'hold' ? 'text-danger' : 'text-ink-2'}`} aria-hidden="true" />
      <div className="min-w-0">{children}</div>
    </div>
  );
}
