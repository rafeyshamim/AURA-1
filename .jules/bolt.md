## 2024-05-20 - Scalar Math Optimization
**Learning:** Using `numpy` functions (`np.hypot`, `np.clip`) for scalar operations is an anti-pattern in this codebase's Python modules (like the high-frequency 30 FPS `hand_tracker.py`). Python's built-in `math.hypot` and `min`/`max` functions are approximately 10x faster for scalars.
**Action:** Use `math.hypot` instead of `np.hypot` and built-in `min`/`max` instead of `np.clip` when dealing with single scalar values in performance-critical Python paths.
