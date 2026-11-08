
## 2024-05-24 - Accessible Interactive Components
**Learning:** Custom interactive UI elements, like the 3D glowing orb, often lack native accessibility out of the box. Adding `role="button"`, `tabindex="0"`, `aria-label`, along with keyboard event listeners (`Enter` / `Space`) and `:focus-visible` styling is essential. Similarly, dynamic text updates like the AURA-1 speech pill need `aria-live` regions to be announced properly to screen reader users without manual focus shifts.
**Action:** When implementing new custom interactive elements, always ensure they are fully navigable via keyboard, have a clear visual focus state, and provide auditory feedback for state changes through appropriate ARIA attributes.


## 2024-05-24 - Dynamic States and ARIA
**Learning:** For highly interactive custom components (like the AURA-1 glowing orb), static ARIA labels are insufficient. As the visual state changes (e.g., from "Listening" to "Processing" to "Speaking"), screen reader users must also receive this context.
**Action:** When components have distinct functional states that are visually apparent, always bind the `aria-label`, `title`, and state attributes like `aria-busy` directly to the state management logic to ensure parity between the visual and accessible experience.

## 2026-11-08 - Preventing Screen Readers from Reading Ghost Text
**Learning:** Using `opacity: 0` alone to visually hide elements (like the AURA-1 speech pill) still leaves them in the accessibility tree, allowing screen readers to announce "ghost" text that sighted users cannot see.
**Action:** Always pair `opacity: 0` with `visibility: hidden` and add a `visibility` transition when fading out interactive elements or live regions to ensure they are properly removed from the accessibility tree when visually hidden.
