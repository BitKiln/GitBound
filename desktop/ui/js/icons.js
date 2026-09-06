// Icon factory. Draws from the <symbol> sprite at the top of index.html, which
// carries Lucide 0.468.0 path data vendored verbatim (ISC — see NOTICE). That
// is the same version the project previously pulled in through lucide-react.
//
// Built with createElementNS rather than a table of SVG source strings, because
// the string approach would need innerHTML — and this codebase's XSS guarantee
// rests on innerHTML appearing nowhere at all.
const SVG_NS = "http://www.w3.org/2000/svg";

// fill/stroke/stroke-width/stroke-linecap/stroke-linejoin are inherited
// presentation properties, so setting them on the outer <svg> reaches the
// <symbol> content across the <use> shadow boundary. Setting them on the
// symbols instead would not work.
const PRESENTATION = {
  fill: "none",
  stroke: "currentColor",
  "stroke-width": "2",
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
};

/**
 * An <svg> referencing sprite symbol `#i-<name>`. `name` always comes from a
 * call site in this codebase, never from backend or user data.
 * Decorative by default; pass a `label` for a standalone meaningful icon.
 */
export function icon(name, size = 24, label) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 24 24");
  for (const [key, value] of Object.entries(PRESENTATION)) svg.setAttribute(key, value);
  if (label) {
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", label);
  } else {
    svg.setAttribute("aria-hidden", "true");
  }
  const use = document.createElementNS(SVG_NS, "use");
  // Same-document reference only. An external "sprite.svg#id" would be blocked
  // by the content security policy.
  use.setAttribute("href", `#i-${name}`);
  svg.append(use);
  return svg;
}

/** The spinner: same factory, plus the class app.css animates. */
export function spinner(size = 24) {
  const svg = icon("loader-circle", size);
  svg.setAttribute("class", "icon spin");
  return svg;
}
