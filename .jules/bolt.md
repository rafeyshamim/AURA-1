## 2024-05-20 - Scalar Math Optimization
**Learning:** Using `numpy` functions (`np.hypot`, `np.clip`) for scalar operations is an anti-pattern in this codebase's Python modules (like the high-frequency 30 FPS `hand_tracker.py`). Python's built-in `math.hypot` and `min`/`max` functions are approximately 10x faster for scalars.
**Action:** Use `math.hypot` instead of `np.hypot` and built-in `min`/`max` instead of `np.clip` when dealing with single scalar values in performance-critical Python paths.

## 2023-11-21 - Optimize Rust Log Buffer with VecDeque (O(N) -> O(1))
**Learning:** Found an inefficient custom logger buffer implementation where `buf.remove(0)` was used on a `Vec<LogEntry>` up to 500 items deep, imposing O(N) overhead on high-frequency logs like webcam tracker reports.
**Action:** Replaced `Vec` with `std::collections::VecDeque` to achieve O(1) buffer rotation, but remembered to convert back to `Vec` upon extraction to maintain Tauri's command signature compatibility. Also cached `Local::now()` for O(1) time query.

## 2025-02-23 - RMS Calculation Optimization with NumPy Arrays
**Learning:** Found an inefficient root-mean-square calculation `np.sqrt(np.mean(chunk**2))` inside a high-frequency audio stream loop (64ms blocks) in `speech_engine.py`. This approach creates unnecessary intermediate arrays in memory.
**Action:** Replaced with `math.sqrt(np.dot(chunk, chunk) / chunk.size)` which uses highly optimized C routines and avoids creating large intermediate arrays, running ~6-7x faster on numpy arrays.

## 2025-02-24 - Pre-Buffer O(N) Array Popping
**Learning:** Found a list used as a FIFO queue (`pre_buffer = []`) with `pre_buffer.pop(0)` in the `speech_engine.py` high-frequency audio loop. Popping the 0th element in a list creates an O(N) memory shift every single block (64ms).
**Action:** Replaced the list with a `collections.deque(maxlen=pre_buffer_size)`. Deque automatically pushes out old items when full, making appending and dropping O(1) without requiring explicit length checks or popping.

## 2025-02-24 - Redundant Array Typing
**Learning:** `np.concatenate(speech_frames).astype(np.float32)` was being called, but `sd.InputStream` already outputs `float32` typed arrays. The `astype(np.float32)` operation was redundantly making a full copy of the audio array.
**Action:** Remove redundant `astype` conversions when concatenating NumPy arrays that are already of the desired type to save memory allocations and CPU time.
