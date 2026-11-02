
## 2024-05-24 - Accessible Interactive Components
**Learning:** Custom interactive UI elements, like the 3D glowing orb, often lack native accessibility out of the box. Adding `role="button"`, `tabindex="0"`, `aria-label`, along with keyboard event listeners (`Enter` / `Space`) and `:focus-visible` styling is essential. Similarly, dynamic text updates like the AURA-1 speech pill need `aria-live` regions to be announced properly to screen reader users without manual focus shifts.
**Action:** When implementing new custom interactive elements, always ensure they are fully navigable via keyboard, have a clear visual focus state, and provide auditory feedback for state changes through appropriate ARIA attributes.


## 2024-05-24 - Dynamic ARIA State and Avoiding Nested `aria-live`
**Learning:** Having an `aria-live` region nested inside another `aria-live` region often causes screen readers to double-announce updates, creating a confusing user experience. Additionally, an interactive element (like the orb) should have its `aria-label` dynamically updated alongside its internal state to correctly reflect what it is currently doing (e.g., listening vs processing).
**Action:** Remove redundant `aria-live` attributes on nested children and ensure JS state management functions (like `setOrbState`) also update relevant accessibility attributes (`aria-label`, `title`) so screen readers stay synced with the visual state.
