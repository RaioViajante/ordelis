import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";

// A confirmation sheet for an action that removes something. The safe choice (Cancel) has focus when it opens
// and is what Escape does. Focus stays inside while it is open; the caller returns focus when it closes.
export function ConfirmDialog({
  title,
  children,
  confirmLabel,
  busy,
  onConfirm,
  onCancel,
}: {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const titleId = useId();
  const bodyId = useId();
  const cancel = useRef<HTMLButtonElement>(null);
  const confirm = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancel.current?.focus();
  }, []);

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (!busy) onCancel();
    } else if (e.key === "Tab") {
      const first = cancel.current;
      const last = confirm.current;
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last?.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first?.focus();
      }
    }
  }

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && !busy && onCancel()}>
      <div role="alertdialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={bodyId} className="sheet" onKeyDown={onKeyDown}>
        <h2 id={titleId} className="sheet__t">
          {title}
        </h2>
        <div id={bodyId} className="sheet__body">
          {children}
        </div>
        <div className="sheet__ft">
          <button type="button" ref={cancel} className="btn" onClick={onCancel} aria-disabled={busy || undefined}>
            Cancel
          </button>
          <button type="button" ref={confirm} className="btn btn--bad" aria-disabled={busy || undefined} onClick={() => !busy && onConfirm()}>
            {busy ? "Removing…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
