import sys
import os
import time
import socket
import signal
import threading
import subprocess

def is_port_in_use(port=8000, host="127.0.0.1"):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(0.6)
        return s.connect_ex((host, port)) == 0

def emit(level, message):
    try:
        import json
        payload = json.dumps({"category": "CHROMA", "level": level, "message": message})
        sys.stdout.write(payload + "\n")
        sys.stdout.flush()
    except Exception:
        pass

is_running = True
server_proc = None

def cleanup():
    global server_proc, is_running
    is_running = False
    if server_proc is not None:
        try:
            server_proc.terminate()
            server_proc.wait(timeout=2)
        except Exception:
            try:
                server_proc.kill()
            except Exception:
                pass
        server_proc = None
    emit("INFO", "ChromaDB server stopped cleanly.")

def sig_handler(signum, frame):
    cleanup()
    os._exit(0)

signal.signal(signal.SIGINT, sig_handler)
signal.signal(signal.SIGTERM, sig_handler)

def parent_watchdog():
    ppid = os.getppid()
    while is_running:
        time.sleep(0.5)
        try:
            current_ppid = os.getppid()
            if current_ppid != ppid or current_ppid <= 1:
                cleanup()
                os._exit(0)
        except Exception:
            cleanup()
            os._exit(0)

def stdin_listener():
    try:
        while is_running:
            line = sys.stdin.readline()
            if not line:
                time.sleep(0.5)
                continue
            if "exit" in line or "stop" in line:
                cleanup()
                os._exit(0)
    except Exception:
        pass

def main():
    global server_proc

    # Start watcher threads
    threading.Thread(target=parent_watchdog, daemon=True).start()
    threading.Thread(target=stdin_listener, daemon=True).start()

    if is_port_in_use(8000):
        emit("SUCCESS", "ChromaDB server is already active on http://127.0.0.1:8000.")
        while is_running:
            time.sleep(1)
        return

    emit("INFO", "Starting ChromaDB server on http://127.0.0.1:8000 (persistence: chroma_db)...")

    # Find chroma command
    chroma_cmd = "chroma"
    chroma_script = os.path.join(os.path.dirname(sys.executable), "Scripts", "chroma.exe")
    if os.path.exists(chroma_script):
        chroma_cmd = chroma_script

    storage_path = os.path.join(os.getcwd(), "chroma_db")
    os.makedirs(storage_path, exist_ok=True)

    cmd = [chroma_cmd, "run", "--path", storage_path, "--port", "8000", "--host", "127.0.0.1"]

    try:
        server_proc = subprocess.Popen(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
        )
    except Exception as e:
        emit("ERROR", f"Failed to start chroma via '{chroma_cmd}': {e}.")
        return

    # Wait for server to become responsive on port 8000
    started = False
    for _ in range(30):
        time.sleep(0.3)
        if is_port_in_use(8000):
            started = True
            break
        if server_proc.poll() is not None:
            break

    if started:
        emit("SUCCESS", "ChromaDB Vector Server online and listening on http://127.0.0.1:8000!")
    else:
        emit("WARN", "ChromaDB server started but port 8000 not responding within timeout.")

    # Keep alive until terminated
    try:
        while is_running:
            if server_proc and server_proc.poll() is not None:
                emit("WARN", f"ChromaDB process exited with code {server_proc.returncode}.")
                break
            time.sleep(1)
    finally:
        cleanup()

if __name__ == "__main__":
    main()
