import { useEffect, useRef, type ReactNode } from "react";

/** Native modal <dialog>: focus stays inside, Esc closes, the page behind is inert. */
export function Modal({
  open,
  onClose,
  labelledBy,
  className = "",
  children,
}: {
  open: boolean;
  onClose: () => void;
  labelledBy: string;
  className?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      // showModal() focuses the first button; start where typing begins instead.
      dialog.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    }
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog ref={ref} className={`modal ${className}`} aria-labelledby={labelledBy} onClose={onClose}>
      {open && children}
    </dialog>
  );
}
