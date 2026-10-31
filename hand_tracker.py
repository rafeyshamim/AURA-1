import sys
import os
import json
import time
import signal
import threading
from collections import deque
import cv2
import numpy as np
import math

def emit(data):
    """Emit JSON payload to Rust backend via stdout."""
    try:
        raw = (json.dumps(data) + "\n").encode("utf-8")
        sys.stdout.buffer.write(raw)
        sys.stdout.buffer.flush()
    except Exception:
        pass

# Global tracking state & lifecycle controls
is_running = True
is_tracking = False
lock = threading.Lock()
cap = None
latest_frame = None

def cleanup_cv2():
    """Ensure OpenCV camera and any windows are completely released."""
    global cap
    with lock:
        if cap is not None:
            try:
                cap.release()
            except Exception:
                pass
            cap = None
        try:
            cv2.destroyAllWindows()
        except Exception:
            pass
    emit({"type": "log", "level": "INFO", "message": "CV2 camera released cleanly."})

def sig_handler(signum, frame):
    """Handle KeyboardInterrupt (Ctrl+C) and termination signals."""
    global is_running, is_tracking
    with lock:
        is_running = False
        is_tracking = False
    cleanup_cv2()
    os._exit(0)

signal.signal(signal.SIGINT, sig_handler)
signal.signal(signal.SIGTERM, sig_handler)

def parent_watchdog():
    """Watch if parent process terminated. If parent dies, exit immediately to release webcam."""
    ppid = os.getppid()
    while is_running:
        time.sleep(0.5)
        try:
            current_ppid = os.getppid()
            if current_ppid != ppid or current_ppid <= 1:
                cleanup_cv2()
                os._exit(0)
        except Exception:
            cleanup_cv2()
            os._exit(0)

def take_webcam_snapshot():
    """Takes a live high-resolution photo of the user from the webcam and emits as JPEG base64."""
    global cap, latest_frame
    snapshot_path = os.path.join(os.getcwd(), "webcam_snapshot.jpg")
    frame_to_use = None

    with lock:
        if cap is not None and latest_frame is not None:
            frame_to_use = latest_frame.copy()

    # If camera is not currently active, open it temporarily for 1 snapshot
    if frame_to_use is None:
        try:
            temp_cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
            if not temp_cap.isOpened():
                temp_cap = cv2.VideoCapture(0)
            if temp_cap.isOpened():
                # Allow 3 frames for sensor exposure & white balance adjustment
                for _ in range(3):
                    ret, f = temp_cap.read()
                    if ret and f is not None:
                        frame_to_use = f
                temp_cap.release()
        except Exception as e:
            emit({"type": "log", "level": "ERROR", "message": f"Snapshot camera error: {e}"})

    if frame_to_use is not None:
        import base64
        ret, buf = cv2.imencode(".jpg", frame_to_use, [int(cv2.IMWRITE_JPEG_QUALITY), 88])
        if ret:
            b64_str = base64.b64encode(buf).decode('utf-8')
            try:
                with open(snapshot_path, "wb") as f:
                    f.write(buf)
            except Exception:
                pass
            emit({
                "type": "webcam_snapshot",
                "success": True,
                "image_base64": b64_str,
                "path": snapshot_path
            })
            return

    emit({"type": "webcam_snapshot", "success": False, "message": "Could not access webcam for snapshot."})

def get_model_path():
    candidates = [
        os.path.join(os.path.dirname(__file__), "hand_landmarker.task"),
        os.path.join(os.getcwd(), "hand_landmarker.task"),
        os.path.join(os.path.dirname(__file__), "..", "hand_landmarker.task"),
    ]
    for c in candidates:
        if os.path.exists(c):
            return os.path.abspath(c)
    return None

def stdin_listener():
    """Listens for JSON commands on stdin from Rust backend."""
    global is_running, is_tracking
    try:
        if hasattr(sys.stdin, "reconfigure"):
            sys.stdin.reconfigure(line_buffering=True)
    except Exception:
        pass

    while is_running:
        try:
            line = sys.stdin.readline()
            if not line:
                # Parent process closed stdin or crashed -> terminate cleanly and immediately
                cleanup_cv2()
                os._exit(0)
            line = line.strip()
            if not line:
                continue
            data = json.loads(line)
            cmd = data.get("command")
            if cmd == "start":
                with lock:
                    is_tracking = True
                emit({"type": "status", "tracking": True, "message": "Hand tracking & gesture detection started."})
            elif cmd == "stop":
                with lock:
                    is_tracking = False
                cleanup_cv2()
                emit({"type": "status", "tracking": False, "message": "Hand tracking stopped. Camera released."})
            elif cmd == "snapshot":
                take_webcam_snapshot()
            elif cmd == "exit":
                with lock:
                    is_running = False
                    is_tracking = False
                cleanup_cv2()
                os._exit(0)
        except (KeyboardInterrupt, SystemExit):
            cleanup_cv2()
            os._exit(0)
        except Exception as e:
            emit({"type": "log", "level": "WARN", "message": f"Stdin listener error: {e}"})

def dist(p1, p2):
    return math.hypot(p1.x - p2.x, p1.y - p2.y)

def main():
    global is_running, is_tracking, cap, latest_frame

    # Start stdin listener thread immediately so commands and exits are handled with zero delay
    t_stdin = threading.Thread(target=stdin_listener, daemon=True)
    t_stdin.start()

    # Start parent watchdog thread immediately to prevent orphan background camera access
    t_watch = threading.Thread(target=parent_watchdog, daemon=True)
    t_watch.start()

    emit({"type": "log", "level": "INFO", "message": "Initializing AURA Computer Vision & Gesture Recognition..."})

    model_path = get_model_path()
    if not model_path:
        emit({"type": "log", "level": "ERROR", "message": "hand_landmarker.task not found."})
        return

    try:
        import mediapipe as mp
        from mediapipe.tasks import python
        from mediapipe.tasks.python import vision

        base_options = python.BaseOptions(model_asset_path=model_path)
        options = vision.HandLandmarkerOptions(
            base_options=base_options,
            num_hands=1,
            min_hand_detection_confidence=0.5,
            min_hand_presence_confidence=0.5,
            min_tracking_confidence=0.5
        )
        detector = vision.HandLandmarker.create_from_options(options)
        emit({"type": "ready", "message": "MediaPipe Hand Landmarker & Gesture Engine ready."})
    except Exception as e:
        emit({"type": "log", "level": "ERROR", "message": f"Failed to load MediaPipe HandLandmarker: {e}"})
        return

    # Smoothing and trajectory state
    smooth_x = 0.5
    smooth_y = 0.5
    deadzone = 0.10

    # Sliding history for swipe detection: (mirrored_x, y, timestamp)
    pos_history = deque(maxlen=10)
    last_swipe_time = 0.0
    swipe_cooldown = 0.55

    # Posture gesture state & cooldown
    last_posture = None
    last_posture_time = 0.0
    posture_cooldown = 0.90

    try:
        while is_running:
            with lock:
                tracking_active = is_tracking

            if not tracking_active:
                if cap is not None:
                    cleanup_cv2()
                time.sleep(0.08)
                continue

            # Open camera if not active
            if cap is None:
                try:
                    cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
                    if not cap.isOpened():
                        cap = cv2.VideoCapture(0)
                    if not cap.isOpened():
                        emit({"type": "log", "level": "ERROR", "message": "Failed to open webcam Camera 0."})
                        with lock:
                            is_tracking = False
                        time.sleep(1.0)
                        continue
                    cap.set(cv2.CAP_PROP_FRAME_WIDTH, 640)
                    cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 480)
                    cap.set(cv2.CAP_PROP_FPS, 30)
                    emit({"type": "log", "level": "INFO", "message": "Webcam stream active (Vision & Gesture AI)."})
                except Exception as e:
                    emit({"type": "log", "level": "ERROR", "message": f"Camera init error: {e}"})
                    with lock:
                        is_tracking = False
                    time.sleep(1.0)
                    continue

            ret, frame = cap.read()
            if not ret or frame is None:
                time.sleep(0.02)
                continue

            with lock:
                latest_frame = frame.copy()

            now = time.time()

            try:
                rgb_frame = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
                mp_image = mp.Image(image_format=mp.ImageFormat.SRGB, data=rgb_frame)
                result = detector.detect(mp_image)

                if result.hand_landmarks and len(result.hand_landmarks) > 0:
                    lm = result.hand_landmarks[0]
                    wrist = lm[0]
                    idx_tip = lm[8]
                    mid_mcp = lm[9]

                    palm_size = max(0.04, dist(wrist, mid_mcp))

                    palm_center_x = (wrist.x + mid_mcp.x) * 0.5
                    palm_center_y = (wrist.y + mid_mcp.y) * 0.5
                    mirrored_palm_x = 1.0 - palm_center_x

                    pos_history.append((mirrored_palm_x, palm_center_y, now))

                    # 1. Dynamic Swipe Detection
                    if len(pos_history) >= 4 and (now - last_swipe_time) > swipe_cooldown:
                        old_x, old_y, old_t = pos_history[0]
                        dx = mirrored_palm_x - old_x
                        dy = palm_center_y - old_y
                        dt = max(0.04, now - old_t)

                        vx = dx / dt
                        vy = dy / dt

                        if abs(dx) > 0.16 and abs(vx) > 0.80 and abs(vx) > (1.5 * abs(vy)):
                            if dx > 0:
                                emit({"type": "gesture", "gesture": "swipe_right"})
                                last_swipe_time = now
                                pos_history.clear()
                            else:
                                emit({"type": "gesture", "gesture": "swipe_left"})
                                last_swipe_time = now
                                pos_history.clear()
                        elif abs(dy) > 0.14 and abs(vy) > 0.70 and abs(vy) > (1.5 * abs(vx)):
                            if dy < 0:
                                emit({"type": "gesture", "gesture": "swipe_up"})
                                last_swipe_time = now
                                pos_history.clear()
                            else:
                                emit({"type": "gesture", "gesture": "swipe_down"})
                                last_swipe_time = now
                                pos_history.clear()

                    # 2. Static Posture Gesture Recognition
                    thumb_ext = dist(wrist, lm[4]) > dist(wrist, lm[2]) * 1.15
                    index_ext = dist(wrist, lm[8]) > dist(wrist, lm[6]) * 1.18
                    mid_ext = dist(wrist, lm[12]) > dist(wrist, lm[10]) * 1.18
                    ring_ext = dist(wrist, lm[16]) > dist(wrist, lm[14]) * 1.18
                    pinky_ext = dist(wrist, lm[20]) > dist(wrist, lm[18]) * 1.18

                    pinch_ratio = dist(lm[4], lm[8]) / palm_size

                    detected_posture = None

                    if pinch_ratio < 0.28:
                        detected_posture = "pinch"
                    elif not index_ext and not mid_ext and not ring_ext and not pinky_ext:
                        detected_posture = "fist"
                    elif index_ext and mid_ext and not ring_ext and not pinky_ext:
                        detected_posture = "peace"
                    elif thumb_ext and not index_ext and not mid_ext and not ring_ext and not pinky_ext and (lm[4].y < lm[2].y):
                        detected_posture = "thumbs_up"
                    elif index_ext and mid_ext and ring_ext and pinky_ext:
                        detected_posture = "open_palm"

                    if detected_posture and (detected_posture != last_posture or (now - last_posture_time) > posture_cooldown):
                        emit({"type": "gesture", "gesture": detected_posture})
                        last_posture = detected_posture
                        last_posture_time = now
                    elif detected_posture is None:
                        last_posture = None

                    # 3. Butter-Smooth Real-Time Coordinate Tracking
                    raw_x = idx_tip.x
                    raw_y = idx_tip.y
                    mirrored_x = 1.0 - raw_x

                    clamped_x = (mirrored_x - deadzone) / (1.0 - 2.0 * deadzone)
                    clamped_y = (raw_y - deadzone) / (1.0 - 2.0 * deadzone)
                    clamped_x = max(0.0, min(1.0, clamped_x))
                    clamped_y = max(0.0, min(1.0, clamped_y))

                    displacement = math.hypot(clamped_x - smooth_x, clamped_y - smooth_y)
                    alpha = float(max(0.16, min(0.44, 0.16 + displacement * 1.5)))

                    smooth_x = alpha * clamped_x + (1.0 - alpha) * smooth_x
                    smooth_y = alpha * clamped_y + (1.0 - alpha) * smooth_y

                    emit({
                        "type": "hand_move",
                        "norm_x": round(float(smooth_x), 4),
                        "norm_y": round(float(smooth_y), 4),
                        "detected": True
                    })
                else:
                    pos_history.clear()
                    emit({
                        "type": "hand_move",
                        "detected": False
                    })
            except Exception as e:
                emit({"type": "log", "level": "WARN", "message": f"Detection error: {e}"})

            time.sleep(0.024)
    except (KeyboardInterrupt, SystemExit):
        pass
    finally:
        cleanup_cv2()

if __name__ == "__main__":
    main()
