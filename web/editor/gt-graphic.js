export class GtGraphic extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this._root = document.createElement("div");
    this.shadowRoot.append(this._root);
    this.selected = null;
    this._root.addEventListener("click", (event) => {
      const target = event.target.closest("[data-gt-name]");
      this.selected = target?.getAttribute("data-gt-name") ?? null;
      this.dispatchEvent(new CustomEvent("gt-select", { detail: this.selected, bubbles: true }));
    });
  }

  setHtml(html) {
    const parsed = new DOMParser().parseFromString(html, "text/html");
    const stage = parsed.querySelector(".gt-stage") || parsed.body;
    this._root.replaceChildren(stage.cloneNode(true));
  }

  setField(name, text) {
    const node = this._root.querySelector(`[data-gt-name="${CSS.escape(name)}"]`);
    if (node) {
      const textNode = node.querySelector(".gt-text") || node;
      textNode.textContent = text;
    }
  }

  applyOverrides(frame) {
    const objects = frame?.objects || {};
    for (const el of this._root.querySelectorAll("[data-gt-name]")) {
      const name = el.getAttribute("data-gt-name");
      const over = objects[name];
      if (!over) {
        el.style.removeProperty("transform");
        el.style.removeProperty("opacity");
        el.style.removeProperty("clip-path");
        continue;
      }
      if (over.hidden) {
        el.style.visibility = "hidden";
      } else {
        el.style.visibility = "";
      }
      el.style.opacity = String(over.opacity_mul);
      const transforms = [];
      if (over.offset_x || over.offset_y) {
        transforms.push(`translate(${over.offset_x}px, ${over.offset_y}px)`);
      }
      if (over.scale_x !== 1 || over.scale_y !== 1) {
        transforms.push(`scale(${over.scale_x}, ${over.scale_y})`);
      }
      if (over.rotate_z) {
        transforms.push(`rotate(${over.rotate_z * 180 / Math.PI}deg)`);
      }
      el.style.transform = transforms.join(" ");
      if (over.has_crop) {
        const top = over.crop_y0 * 100;
        const right = (1 - over.crop_x1) * 100;
        const bottom = (1 - over.crop_y1) * 100;
        const left = over.crop_x0 * 100;
        el.style.clipPath = `inset(${top}% ${right}% ${bottom}% ${left}%)`;
      }
    }
  }

  clearOverrides() {
    this.applyOverrides({ objects: {} });
  }
}

customElements.define("gt-graphic", GtGraphic);
