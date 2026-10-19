// Minimal DOM helpers; the UI is plain TypeScript without a framework.

type Attrs = Record<string, string | number | boolean | EventListener | null | undefined>;
type Child = Node | string | null | undefined | false;

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  ...children: (Child | Child[])[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value == null || value === false) continue;
    if (key.startsWith("on") && typeof value === "function") {
      el.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (key === "class") {
      el.className = String(value);
    } else if (key in el && key !== "list" && key !== "form") {
      (el as unknown as Record<string, unknown>)[key] = value;
    } else {
      el.setAttribute(key, value === true ? "" : String(value));
    }
  }
  for (const child of children.flat()) {
    if (child == null || child === false) continue;
    el.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
  return el;
}

export function $(selector: string, root: ParentNode = document): HTMLElement {
  const el = root.querySelector<HTMLElement>(selector);
  if (!el) throw new Error(`missing element ${selector}`);
  return el;
}

export function toast(message: string, kind: "info" | "error" = "info"): void {
  let host = document.querySelector<HTMLElement>(".toasts");
  if (!host) {
    host = h("div", { class: "toasts", role: "status", "aria-live": "polite" });
    document.body.append(host);
  }
  const el = h("div", { class: `toast ${kind}` }, message);
  host.append(el);
  setTimeout(() => el.classList.add("leaving"), kind === "error" ? 7000 : 3500);
  setTimeout(() => el.remove(), kind === "error" ? 7400 : 3900);
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** Runs `fn` at most once per animation frame with the latest arguments. */
export function perFrame<A extends unknown[]>(fn: (...args: A) => void): (...args: A) => void {
  let pending: A | null = null;
  return (...args: A) => {
    const first = pending === null;
    pending = args;
    if (first) {
      requestAnimationFrame(() => {
        const a = pending!;
        pending = null;
        fn(...a);
      });
    }
  };
}

export function debounce<A extends unknown[]>(fn: (...args: A) => void, ms: number): (...args: A) => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return (...args: A) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

/** `replaceChildren` that skips null/false entries, like `h`. */
export function fill(el: Element, ...children: (Child | Child[])[]): void {
  el.replaceChildren(...children.flat().filter((c): c is Node | string => c != null && c !== false));
}
