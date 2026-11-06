## 2024-05-20 - Scalar Math Optimization
**Learning:** Using `numpy` functions (`np.hypot`, `np.clip`) for scalar operations is an anti-pattern in this codebase's Python modules (like the high-frequency 30 FPS `hand_tracker.py`). Python's built-in `math.hypot` and `min`/`max` functions are approximately 10x faster for scalars.
**Action:** Use `math.hypot` instead of `np.hypot` and built-in `min`/`max` instead of `np.clip` when dealing with single scalar values in performance-critical Python paths.

## 2023-11-21 - Optimize Rust Log Buffer with VecDeque (O(N) -> O(1))
**Learning:** Found an inefficient custom logger buffer implementation where `buf.remove(0)` was used on a `Vec<LogEntry>` up to 500 items deep, imposing O(N) overhead on high-frequency logs like webcam tracker reports.
**Action:** Replaced `Vec` with `std::collections::VecDeque` to achieve O(1) buffer rotation, but remembered to convert back to `Vec` upon extraction to maintain Tauri's command signature compatibility. Also cached `Local::now()` for O(1) time query.

## 2025-02-23 - RMS Calculation Optimization with NumPy Arrays
**Learning:** Found an inefficient root-mean-square calculation `np.sqrt(np.mean(chunk**2))` inside a high-frequency audio stream loop (64ms blocks) in `speech_engine.py`. This approach creates unnecessary intermediate arrays in memory.
**Action:** Replaced with `math.sqrt(np.dot(chunk, chunk) / chunk.size)` which uses highly optimized C routines and avoids creating large intermediate arrays, running ~6-7x faster on numpy arrays.

## 2026-11-06 - Optimize High-Frequency Audio Stream Buffer with Deque
**Learning:** Found an inefficient O(N) list shift operation `pre_buffer.pop(0)` being called repeatedly in the high-frequency (64ms blocks) audio stream processing loop of `speech_engine.py`. For sliding windows, Python lists impose an O(N) memory shift penalty when popping from the beginning.
**Action:** Replaced the standard list with `collections.deque(maxlen=N)`. The `deque` structure naturally handles dropping the oldest item when `maxlen` is reached, providing an O(1) rotation and eliminating the manual O(N) pop operation.
