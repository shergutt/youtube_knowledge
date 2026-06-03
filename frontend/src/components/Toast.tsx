import { useEffect, useState } from "react";

export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

let nextId = 1;

type Listener = (toasts: Toast[]) => void;
const listeners = new Set<Listener>();
let toasts: Toast[] = [];

function emit() {
  for (const l of listeners) l([...toasts]);
}

export function showToast(message: string, kind: ToastKind = "info", ttlMs = 3500) {
  const id = nextId++;
  toasts = [...toasts, { id, kind, message }];
  emit();
  setTimeout(() => {
    toasts = toasts.filter((t) => t.id !== id);
    emit();
  }, ttlMs);
}

export function ToastHost() {
  const [items, setItems] = useState<Toast[]>(toasts);
  useEffect(() => {
    listeners.add(setItems);
    return () => {
      listeners.delete(setItems);
    };
  }, []);
  return (
    <div className="toast-host" role="status" aria-live="polite">
      {items.map((t) => (
        <div key={t.id} className={`toast toast-${t.kind}`}>
          {t.message}
        </div>
      ))}
    </div>
  );
}
