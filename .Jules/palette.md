
## 2024-05-24 - Accessible Interactive Components
**Learning:** Custom interactive UI elements, like the 3D glowing orb, often lack native accessibility out of the box. Adding `role="button"`, `tabindex="0"`, `aria-label`, along with keyboard event listeners (`Enter` / `Space`) and `:focus-visible` styling is essential. Similarly, dynamic text updates like the AURA-1 speech pill need `aria-live` regions to be announced properly to screen reader users without manual focus shifts.
**Action:** When implementing new custom interactive elements, always ensure they are fully navigable via keyboard, have a clear visual focus state, and provide auditory feedback for state changes through appropriate ARIA attributes.


## 2024-05-24 - Dynamic States and ARIA
**Learning:** For highly interactive custom components (like the AURA-1 glowing orb), static ARIA labels are insufficient. As the visual state changes (e.g., from "Listening" to "Processing" to "Speaking"), screen reader users must also receive this context.
**Action:** When components have distinct functional states that are visually apparent, always bind the `aria-label`, `title`, and state attributes like `aria-busy` directly to the state management logic to ensure parity between the visual and accessible experience.

## 2025-02-23 - Keyboard Interaction Hints & Screen Reader Cleanliness
**Learning:** Screen reader users can benefit greatly from simple keyboard interaction hints in the `aria-label` or `title` (e.g. "Press Enter to interact") for custom interactive components that don't look like standard buttons. Furthermore, decorative elements (like the animated dot in `aria-live` region) can cause screen reader clutter if not explicitly hidden. Finally, `aria-describedby` provides a powerful semantic link between a control and its dynamically updated caption/subtitle text.
**Action:** When creating custom widget controls, ensure a semantic `main` or landmark region is present. Explicitly add `aria-hidden="true"` to visual flair within live regions. Use `aria-describedby` to associate custom controls with their real-time readout text.
