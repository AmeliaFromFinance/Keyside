// Keyside overlay for OBS browser sources. Shows text the companion fills in and streams
// to the page. Options come from the address, like ?size=96&color=ffcc00&font=Inter&hide=1.
(() => {
  const q = new URLSearchParams(location.search);
  const out = document.getElementById("out");
  const timer = location.pathname.match(/^\/overlay\/timer\/([a-z0-9_-]{1,24})\/?$/);
  const src = timer ? `/overlay/events?timer=${timer[1]}` : `/overlay/events?t=${encodeURIComponent(q.get("t") ?? "")}`;
  document.body.classList.add(timer ? "timer" : "text");

  const size = Number(q.get("size"));
  if (size >= 8 && size <= 1000) out.style.fontSize = `${size}px`;
  const color = (q.get("color") ?? q.get("colour") ?? "").trim();
  if (/^#?([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(color)) out.style.color = color.startsWith("#") ? color : `#${color}`;
  else if (/^[a-z]{3,24}$/i.test(color)) out.style.color = color;
  const font = (q.get("font") ?? "").trim();
  if (/^[\w .-]{1,60}$/.test(font)) out.style.fontFamily = `"${font}", system-ui, sans-serif`;
  const weight = (q.get("weight") ?? "").trim();
  if (/^([1-9]00|normal|bold)$/.test(weight)) out.style.fontWeight = weight;
  const align = q.get("align");
  if (align === "left" || align === "center" || align === "right") out.style.textAlign = align;
  if (q.get("shadow") === "0") out.classList.add("flat");
  const hide = q.get("hide") === "1";

  const events = new EventSource(src);
  events.onmessage = (e) => {
    let d;
    try {
      d = JSON.parse(e.data);
    } catch {
      return;
    }
    out.textContent = typeof d.text === "string" ? d.text : "";
    const hidden = hide && (timer ? !d.running : !out.textContent.trim());
    out.style.visibility = hidden ? "hidden" : "visible";
  };
})();
