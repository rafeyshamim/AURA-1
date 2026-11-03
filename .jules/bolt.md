## 2024-05-20 - Scalar Math Optimization
**Learning:** Using `numpy` functions (`np.hypot`, `np.clip`) for scalar operations is an anti-pattern in this codebase's Python modules (like the high-frequency 30 FPS `hand_tracker.py`). Python's built-in `math.hypot` and `min`/`max` functions are approximately 10x faster for scalars.
**Action:** Use `math.hypot` instead of `np.hypot` and built-in `min`/`max` instead of `np.clip` when dealing with single scalar values in performance-critical Python paths.

## 2023-11-21 - Optimize Rust Log Buffer with VecDeque (O(N) -> O(1))
**Learning:** Found an inefficient custom logger buffer implementation where `buf.remove(0)` was used on a `Vec<LogEntry>` up to 500 items deep, imposing O(N) overhead on high-frequency logs like webcam tracker reports.
**Action:** Replaced `Vec` with `std::collections::VecDeque` to achieve O(1) buffer rotation, but remembered to convert back to `Vec` upon extraction to maintain Tauri's command signature compatibility. Also cached `Local::now()` for O(1) time query.

## 2024-05-21 - Vectorized RMS Energy Optimization
**Learning:** In audio processing loops (like `speech_engine.py`), calculating RMS energy via `np.sqrt(np.mean(chunk**2))` creates a temporary intermediate array (`chunk**2`), which is a major performance overhead on high-frequency chunk streams. Using `np.linalg.norm(chunk) / math.sqrt(len(chunk))` avoids this allocation and leverages LAPACK underneath, making it approximately 4.5x faster.
**Action:** Always prefer `np.linalg.norm` over manual sum-of-squares (`**2`) when calculating RMS or L2 norms for numpy arrays. Use `math.sqrt(len(array))` instead of hardcoded scalars for safety if the array length can vary at the end of streams.

## 2024-05-21 - Vectorized RMS Energy Optimization
**Learning:** In audio processing loops (like `speech_engine.py`), calculating RMS energy via `np.sqrt(np.mean(chunk**2))` creates a temporary intermediate array (`chunk**2`), which is a major performance overhead on high-frequency chunk streams. Using `np.linalg.norm(chunk) / math.sqrt(len(chunk))` avoids this allocation and leverages LAPACK underneath, making it approximately 4.5x faster.
**Action:** Always prefer `np.linalg.norm` over manual sum-of-squares (`**2`) when calculating RMS or L2 norms for numpy arrays. Use `math.sqrt(len(array))` instead of hardcoded scalars for safety if the array length can vary at the end of streams.
