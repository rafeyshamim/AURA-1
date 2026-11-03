
## 2024-05-24 - Accessible Interactive Components
**Learning:** Custom interactive UI elements, like the 3D glowing orb, often lack native accessibility out of the box. Adding `role="button"`, `tabindex="0"`, `aria-label`, along with keyboard event listeners (`Enter` / `Space`) and `:focus-visible` styling is essential. Similarly, dynamic text updates like the AURA-1 speech pill need `aria-live` regions to be announced properly to screen reader users without manual focus shifts.
**Action:** When implementing new custom interactive elements, always ensure they are fully navigable via keyboard, have a clear visual focus state, and provide auditory feedback for state changes through appropriate ARIA attributes.


## 2024-10-25 - Live Regions and Dynamic ARIA Labels
**Learning:** Screen readers often announce `aria-live` regions redundantly if nested incorrectly or if duplicate attributes (`aria-live="polite" aria-atomic="true"`) exist on both the parent container and the child content element. Furthermore, custom stateful elements (like the squinting orb) require their `aria-label` and `title` to be dynamically updated via JavaScript as their state changes, otherwise screen reader and mouse users receive stale context.
**Action:** When implementing live regions, apply `role="status"` (or a single `aria-live` attribute) only to the parent container. Use `aria-hidden="true"` on decorative status dots to prevent clutter. For stateful UI elements, ensure their `aria-label` and `title` are updated synchronously with state transitions in JavaScript.
