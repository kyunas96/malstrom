import type { ReactNode } from 'react';

export function Modal({
  onClose,
  labelledBy,
  children,
}: {
  onClose: () => void;
  labelledBy: string;
  children: ReactNode;
}) {
  return (
    <div className="pull-confirm-backdrop" onClick={onClose}>
      <div
        className="pull-confirm-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}
