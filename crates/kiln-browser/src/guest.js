// Runs in every document, in a world of its own that the page's scripts
// cannot reach, and reports what frames do not show.
(() => {
  const MAX_CHOICES = 1000;
  const send = (message) => kilnSend(JSON.stringify(message));

  let picked = null;

  // Other selects are drawn by the page itself.
  const drawnBySystem = (element) =>
    element instanceof HTMLSelectElement &&
    !element.disabled && !element.multiple && element.size <= 1 &&
    getComputedStyle(element).appearance !== 'base-select';

  const choices = (select) =>
    [...select.options].slice(0, MAX_CHOICES)
      .map((option, index) => ({
        index,
        label: option.label,
        group: option.parentElement instanceof HTMLOptGroupElement
          ? option.parentElement.label : null,
        disabled: option.disabled || option.parentElement.disabled === true,
        hidden: option.hidden || getComputedStyle(option).display === 'none',
      }))
      .filter((choice) => !choice.hidden);

  // Where the document is in the page, as far as frames of one site tell.
  const origin = () => {
    let x = 0, y = 0;
    for (let view = window; view !== view.parent;) {
      let frame;
      try { frame = view.frameElement; } catch { break; }
      if (!frame) break;
      const box = frame.getBoundingClientRect();
      x += box.left + frame.clientLeft;
      y += box.top + frame.clientTop;
      view = view.parent;
    }
    return { x, y };
  };

  // From the pointer where there is one: it is right in any frame.
  const offer = (select, event) => {
    picked = select;
    const box = select.getBoundingClientRect();
    const pointer = event instanceof MouseEvent;
    const from = pointer ? { x: -event.clientX, y: -event.clientY } : origin();
    const scale = visualViewport.scale;
    send({
      select: {
        choices: choices(select),
        selected: select.selectedIndex,
        fromPointer: pointer,
        x: (box.left + from.x) * scale,
        y: (box.bottom + from.y) * scale,
        width: box.width * scale,
      },
    });
  };

  globalThis.kilnChoose = (index) => {
    const select = picked;
    picked = null;
    if (!select || !select.isConnected || select.selectedIndex === index) return;
    select.selectedIndex = index;
    select.dispatchEvent(new Event('input', { bubbles: true, composed: true }));
    select.dispatchEvent(new Event('change', { bubbles: true }));
  };

  addEventListener('mousedown', (event) => {
    const select = event.composedPath().find(drawnBySystem);
    if (!event.isTrusted || event.button !== 0 || !select) return;
    event.preventDefault();
    select.focus();
    offer(select, event);
  }, true);

  addEventListener('keydown', (event) => {
    const select = event.composedPath()[0];
    const opens = [' ', 'ArrowUp', 'ArrowDown'].includes(event.key);
    const held = event.metaKey || event.ctrlKey;
    if (!event.isTrusted || !opens || held || !drawnBySystem(select)) return;
    event.preventDefault();
    offer(select, event);
  }, true);

  const overText = (event) => {
    const caret = document.caretPositionFromPoint(event.clientX, event.clientY);
    if (!(caret?.offsetNode instanceof Text)) return false;
    const range = document.createRange();
    range.selectNodeContents(caret.offsetNode);
    return [...range.getClientRects()].some((box) =>
      box.left <= event.clientX && event.clientX <= box.right &&
      box.top <= event.clientY && event.clientY <= box.bottom);
  };

  let cursor = null;
  addEventListener('mousemove', (event) => {
    const element = event.composedPath()[0];
    if (!(element instanceof Element)) return;
    let now = getComputedStyle(element).cursor.split(',').pop().trim();
    if (now === 'auto') now = overText(event) ? 'text' : 'default';
    if (now === cursor) return;
    cursor = now;
    send({ cursor });
  }, true);
  // What another frame showed meanwhile is not known here.
  addEventListener('mouseout', (event) => {
    if (!event.relatedTarget) cursor = null;
  }, true);

  addEventListener('contextmenu', (event) => {
    if (!event.isTrusted || event.defaultPrevented) return;
    const path = event.composedPath();
    const find = (kind) => path.find((element) => element instanceof kind);
    const field = path[0];
    send({
      menu: {
        link: find(HTMLAnchorElement)?.href || null,
        image: find(HTMLImageElement)?.currentSrc || null,
        editable: field.isContentEditable === true ||
          field instanceof HTMLTextAreaElement ||
          (field instanceof HTMLInputElement && field.selectionStart !== null),
      },
    });
  });
})();
