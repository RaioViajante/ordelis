import { ChevronDown } from "lucide-react";
import { useId, useState, type ButtonHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from "react";

type Variant = "default" | "go" | "ghost" | "link";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "disabled"> {
  variant?: Variant;
  small?: boolean;
  /** When set, the control is disabled and the reason is shown on hover and keyboard focus. */
  reason?: string;
}

export function Button({ variant = "default", small, reason, className = "", children, ...rest }: ButtonProps) {
  const id = useId();
  // Flip the reason to the control's right edge when it would run off the window.
  const [flip, setFlip] = useState(false);
  const cls = [
    "btn",
    variant === "go" && "btn--go",
    variant === "ghost" && "btn--ghost",
    variant === "link" && "btn--link",
    small && "btn--sm",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  if (!reason) {
    return (
      <button type="button" className={cls} {...rest}>
        {children}
      </button>
    );
  }
  return (
    <span
      className={`has-reason${flip ? " has-reason--flip" : ""}`}
      tabIndex={0}
      onMouseEnter={(e) => setFlip(e.currentTarget.getBoundingClientRect().left + 300 > window.innerWidth)}
      onFocus={(e) => setFlip(e.currentTarget.getBoundingClientRect().left + 300 > window.innerWidth)}
    >
      <button type="button" className={cls} {...rest} aria-describedby={id} disabled>
        {children}
      </button>
      <span role="tooltip" id={id} className="reason">
        {reason}
      </span>
    </span>
  );
}

export type EvidenceLevel = "ver" | "adv" | "unk" | "bad";

export function Evidence({ level, children }: { level: EvidenceLevel; children: ReactNode }) {
  return <span className={`ev-l ev--${level}`}>{children}</span>;
}

export function Vision({ children }: { children: ReactNode }) {
  return <span className="vision">{children}</span>;
}

export type AgentId = "cc" | "cx" | "oc" | "km";

export function Agent({ id, children }: { id: AgentId; children: ReactNode }) {
  return <span className={`ag ag--${id}`}>{children}</span>;
}

export function SecHead({ title, meta, children }: { title: ReactNode; meta?: ReactNode; children?: ReactNode }) {
  return (
    <div className="sec-h">
      <h2>{title}</h2>
      {meta && <span className="meta">{meta}</span>}
      {children}
    </div>
  );
}

export function Empty({
  title,
  children,
  why,
  actions,
  small,
}: {
  title: string;
  children: ReactNode;
  why?: string;
  actions?: ReactNode;
  small?: boolean;
}) {
  return (
    <section className={`empty${small ? " empty--sm" : ""}`}>
      <h3>{title}</h3>
      <p>{children}</p>
      {why && <p className="why">{why}</p>}
      {actions && <div className="actions">{actions}</div>}
    </section>
  );
}

export function PageTitle({ children, lede }: { children: ReactNode; lede?: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h1 className="h1">{children}</h1>
      {lede && <p className="hint">{lede}</p>}
    </div>
  );
}

export function Select({ className = "", children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className="select">
      <select className={`field ${className}`.trim()} {...rest}>
        {children}
      </select>
      <ChevronDown size={14} aria-hidden="true" />
    </span>
  );
}
