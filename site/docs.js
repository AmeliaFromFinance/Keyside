// Docs pages: copy buttons on code, the page picker on phones, and the current heading in "On this page".

for (const box of document.querySelectorAll(".doc .code")) {
  const button = document.createElement("button");
  button.className = "copy";
  button.type = "button";
  button.textContent = "Copy";
  button.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(box.querySelector("pre").innerText);
      button.textContent = "Copied";
    } catch {
      button.textContent = "Press Ctrl+C";
    }
    setTimeout(() => (button.textContent = "Copy"), 1600);
  });
  box.append(button);
}

document
  .querySelector(".picker")
  ?.addEventListener("change", (e) => (location.href = e.target.value));

const links = [...document.querySelectorAll(".toc ul a")];
const heads = links
  .map((a) => document.getElementById(decodeURIComponent(a.hash.slice(1))))
  .filter(Boolean);
function mark() {
  let current = heads[0];
  for (const h of heads) if (h.getBoundingClientRect().top < 120) current = h;
  for (const a of links)
    a.classList.toggle("on", a.hash.slice(1) === current?.id);
}
if (heads.length) {
  addEventListener("scroll", mark, { passive: true });
  mark();
}
