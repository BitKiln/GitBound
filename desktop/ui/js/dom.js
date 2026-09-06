// The entire render layer. There is no framework and no build step.
//
// The security property this file exists to guarantee: `h()` is the only way
// DOM is created in this application, and `h()` never parses markup. Text
// becomes a text node, attributes go through setAttribute, and handlers go
// through addEventListener — none of which interpret HTML. Repository paths,
// git author names, remote URLs, and raw git/gh/ssh stderr therefore cannot
// become elements no matter what they contain. There is no escaping helper to
// forget to call, because there is no site that would need one.
//
// CI enforces the other half: no innerHTML, outerHTML, insertAdjacentHTML,
// document.write, eval, or new Function anywhere under ui/js.

/**
 * Build an element. `props` may contain:
 *   class      -> className
 *   on<Event>  -> addEventListener (onClick, onInput, onChange, …)
 *   value / checked / disabled / selected -> set as PROPERTIES, after the
 *     children are appended (see PROPERTY_PROPS)
 *   anything else -> setAttribute
 * `null`, `undefined`, and `false` props and children are skipped, so
 * `cond && h(...)` works as a conditional child the way it does in JSX.
 *
 * There is deliberately no `style` prop: setAttribute("style", …) produces an
 * inline style, which `style-src 'self'` blocks. Styling comes from
 * ui/css/app.css via `class`.
 */
export function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  const deferred = [];
  for (const [key, value] of Object.entries(props ?? {})) {
    if (value == null || value === false) continue;
    if (key === "class") el.className = value;
    else if (key.startsWith("on")) el.addEventListener(key.slice(2).toLowerCase(), value);
    else if (PROPERTY_PROPS.has(key)) deferred.push([key, value]);
    else el.setAttribute(key, value === true ? "" : String(value));
  }
  append(el, kids);
  // Applied after the children exist. `select.value = x` silently does nothing
  // while the element has no <option> to match, which would leave every select
  // showing its first entry regardless of the state it was given.
  for (const [key, value] of deferred) el[key] = value;
  return el;
}

// Set as properties rather than attributes: the matching attributes only
// supply a control's *default*, not its current state.
const PROPERTY_PROPS = new Set(["value", "checked", "disabled", "selected", "indeterminate"]);

/** Append children, flattening arrays and coercing non-nodes to text. */
export function append(el, kids) {
  for (const kid of kids) {
    if (kid == null || kid === false || kid === true) continue;
    if (Array.isArray(kid)) append(el, kid);
    else el.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
  }
}

/** A document fragment, for returning several siblings from one function. */
export function fragment(...kids) {
  const f = document.createDocumentFragment();
  append(f, kids);
  return f;
}

/**
 * A view is `build(state, set)` returning the children of `host`. Calling `set(patch)`
 * merges the patch into state and rebuilds the whole subtree.
 *
 * Rebuilding everything — rather than diffing — is what keeps this a
 * translation of the React original rather than a redesign: useState becomes a
 * key on one state object and setX(v) becomes set({x: v}). At this scale (six
 * views of 50-200 nodes) a full rebuild is sub-millisecond.
 *
 * The one thing a rebuild would otherwise destroy is the focused control and
 * its caret, so every focusable form control MUST carry a stable `data-k`
 * attribute; capture/restore below use it to put the user back where they were.
 * A control without `data-k` loses focus on every keystroke and feels broken.
 * CI greps for this.
 */
export function createView(host, build) {
  let state;
  const set = (patch) => {
    state = { ...state, ...patch };
    draw();
  };
  function draw() {
    const focus = capture(host);
    // `host` is the view's own real element (a <section>, so that the
    // `.workspace > section` rules in app.css still match); `build` returns its
    // children, and may return one node, an array, or a fragment.
    host.replaceChildren();
    append(host, [build(state, set)]);
    restore(host, focus);
  }
  return {
    start(initial) {
      state = initial;
      draw();
    },
    set,
    redraw: draw,
    get state() {
      return state;
    },
  };
}

function capture(host) {
  const el = document.activeElement;
  if (!el || !host.contains(el) || !el.dataset.k) return null;
  return { k: el.dataset.k, start: el.selectionStart, end: el.selectionEnd };
}

function restore(host, focus) {
  if (!focus) return;
  const el = host.querySelector(`[data-k="${CSS.escape(focus.k)}"]`);
  if (!el) return;
  el.focus();
  if (focus.start != null && el.setSelectionRange) {
    // Throws on input types that have no text selection (checkbox, email in
    // some engines). Losing the caret there is harmless; losing focus is not.
    try {
      el.setSelectionRange(focus.start, focus.end);
    } catch {
      /* control has no text selection */
    }
  }
}
