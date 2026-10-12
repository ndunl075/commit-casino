import { useEffect, useRef, type ReactNode } from 'react';

export function Modal({ title, children, onClose, wide = false }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => { const dialog = ref.current; dialog?.showModal(); return () => dialog?.close(); }, []);
  return <dialog ref={ref} className={`casino-modal ${wide ? 'wide-modal' : ''}`} aria-labelledby="modal-title" onCancel={onClose} onClick={event => { if (event.target === ref.current) onClose(); }}>
    <div className="modal-heading"><h2 id="modal-title">{title}</h2><button type="button" className="icon-button" onClick={onClose} aria-label="Close dialog">×</button></div>
    <div className="modal-body">{children}</div>
  </dialog>;
}
