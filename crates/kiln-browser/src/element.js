// Called on a picked element, in the page's own world, where frameworks
// leave what they know about it. All of it is the page's word.
function () {
  const MAX_HTML = 1500, MAX_TEXT = 300, MAX_DEPTH = 6;
  const cut = (text, most) => text.length > most ? text.slice(0, most) + '…' : text;
  const element = this.nodeType === Node.ELEMENT_NODE ? this : this.parentElement;
  if (!element) return null;

  const step = (node) => {
    if (node.id) return '#' + CSS.escape(node.id);
    const tag = node.localName;
    const same = [...(node.parentElement?.children ?? [])].filter((it) => it.localName === tag);
    return same.length > 1 ? `${tag}:nth-of-type(${same.indexOf(node) + 1})` : tag;
  };
  const path = [];
  for (let node = element; node && path.length < MAX_DEPTH; node = node.parentElement) {
    path.unshift(step(node));
    if (node.id) break;
  }

  const framework = () => {
    for (let node = element; node; node = node.parentElement) {
      const key = Object.keys(node).find((it) => it.startsWith('__reactFiber$'));
      for (let fiber = key && node[key]; fiber; fiber = fiber.return) {
        const type = fiber.type;
        const name = typeof type === 'function' && (type.displayName || type.name);
        const source = fiber._debugSource;
        if (name) {
          return { name, source: source ? `${source.fileName}:${source.lineNumber}` : null };
        }
      }
      const vue = node.__vueParentComponent?.type;
      if (vue) return { name: vue.name || vue.__name || null, source: vue.__file || null };
      const svelte = node.__svelte_meta?.loc;
      if (svelte) return { name: null, source: `${svelte.file}:${svelte.line}` };
    }
    return null;
  };
  let known = null;
  try { known = framework(); } catch { /* the page's own objects */ }

  const box = element.getBoundingClientRect();
  return JSON.stringify({
    name: element.localName + [...element.classList].slice(0, 2).map((it) => '.' + it).join(''),
    selector: path.join(' > '),
    text: cut((element.innerText ?? '').trim(), MAX_TEXT),
    html: cut(element.outerHTML, MAX_HTML),
    width: Math.round(box.width),
    height: Math.round(box.height),
    component: known?.name ? String(known.name) : null,
    source: known?.source ? String(known.source) : null,
  });
}
