// Modal dialogs built on <dialog>: a generic frame, progress, and a small inline text prompt.
import { h } from "./dom";

export interface ModalButton {
  label: string;
  value: string;
  primary?: boolean;
}

/** Shows a modal with `body`; resolves with the clicked button's value ("" when dismissed). */
export function modal(title: string, body: (Node | null)[], buttons: ModalButton[], opts: { wide?: boolean; onOpen?: (d: HTMLDialogElement) => void } = {}): Promise<string> {
  return new Promise((resolve) => {
    const dialog = h(
      "dialog",
      { class: `report-dialog ${opts.wide ? "wide" : ""}` },
      h(
        "form",
        { method: "dialog" },
        h("h2", {}, title),
        ...body,
        h(
          "div",
          { class: "dialog-buttons" },
          buttons.map((b) => h("button", { value: b.value, class: b.primary ? "primary" : "", formnovalidate: !b.primary }, b.label)),
        ),
      ),
    );
    dialog.addEventListener("close", () => {
      const v = dialog.returnValue;
      dialog.remove();
      resolve(v);
    });
    document.body.append(dialog);
    dialog.showModal();
    opts.onOpen?.(dialog);
  });
}

export interface Progress {
  set(done: number, total: number, text?: string): void;
  cancelled(): boolean;
  close(): void;
}

/** A non-dismissable progress dialog with a Cancel button. */
export function progress(title: string): Progress {
  let cancelled = false;
  const bar = h("progress", { max: 1, value: 0 });
  const text = h("p", { class: "muted small" }, "");
  const dialog = h(
    "dialog",
    { class: "report-dialog" },
    h("h2", {}, title),
    bar,
    text,
    h("div", { class: "dialog-buttons" }, h("button", { onclick: () => ((cancelled = true), (text.textContent = "Cancelling…")) }, "Cancel")),
  );
  dialog.addEventListener("cancel", (e) => e.preventDefault());
  document.body.append(dialog);
  dialog.showModal();
  return {
    set(done, total, t) {
      bar.max = Math.max(1, total);
      bar.value = done;
      if (t !== undefined && !cancelled) text.textContent = t;
    },
    cancelled: () => cancelled,
    close() {
      dialog.close();
      dialog.remove();
    },
  };
}

/** A one-line text box at a screen position (CSS pixels relative to `host`). */
export function promptAt(host: HTMLElement, at: [number, number], placeholder: string): Promise<string | null> {
  return new Promise((resolve) => {
    const input = h("input", { type: "text", class: "inline-prompt", placeholder });
    input.style.left = `${at[0]}px`;
    input.style.top = `${at[1]}px`;
    let done = false;
    const finish = (v: string | null) => {
      if (done) return;
      done = true;
      input.remove();
      resolve(v && v.trim() ? v.trim() : null);
    };
    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") finish(input.value);
      if (e.key === "Escape") finish(null);
    });
    input.addEventListener("blur", () => finish(input.value));
    host.append(input);
    input.focus();
  });
}

export function field(label: string, input: HTMLElement, hint?: string): HTMLElement {
  return h("label", { class: "field" }, h("span", {}, label), input, hint ? h("small", { class: "muted" }, hint) : null);
}
