import sys
import os
import json
import time
import queue
import threading
import re
import numpy as np
import sounddevice as sd

def emit(data):
    """Emit JSON message to Rust backend via stdout."""
    try:
        raw = (json.dumps(data) + "\n").encode("utf-8")
        sys.stdout.buffer.write(raw)
        sys.stdout.buffer.flush()
    except Exception:
        pass

# Audio Configuration
SAMPLE_RATE = 16000
BLOCK_SIZE = 1024  # ~64ms per block
CHANNELS = 1
VAD_ENERGY_THRESHOLD = 0.016  # RMS threshold for speech
SILENCE_DURATION_SEC = 0.55   # Ultra-fast conversational end-of-speech detection (~550ms)
MIN_SPEECH_DURATION_SEC = 0.40 # Fast trigger for short commands ("yes", "stop", "open")
PRE_BUFFER_SEC = 0.25          # Audio kept before speech trigger

# State
is_speaking = False
is_running = True
audio_queue = queue.Queue()
command_queue = queue.Queue()

# Clean markdown and internal tags for natural TTS reading
def clean_for_tts(text: str) -> str:
    text = re.sub(r'```[\s\S]*?```', '', text) # remove code blocks
    text = re.sub(r'`[^`]*`', '', text)        # remove inline code
    text = re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', text) # markdown links to text
    text = re.sub(r'\[[A-Z_]+:[^\]]*\]', '', text) # remove internal tags like [MOVE:...], [PC:...], [COLOR:...]
    text = re.sub(r'⚠️.*?Error:?', '', text)    # remove warning headers
    text = re.sub(r'https?://\S+', '', text)   # remove URLs
    text = re.sub(r'[*_#~>]', ' ', text)        # remove asterisks, hashes, etc.
    text = re.sub(r'\s+', ' ', text).strip()
    return text

# TTS Engine Initialization
piper_voice = None
pyttsx_engine = None

def init_tts():
    global piper_voice, pyttsx_engine
    candidates = [
        os.path.join(os.path.dirname(__file__), "models", "piper", "en_US-amy-medium.onnx"),
        os.path.join(os.path.dirname(__file__), "..", "models", "piper", "en_US-amy-medium.onnx"),
        os.path.join(os.getcwd(), "models", "piper", "en_US-amy-medium.onnx"),
    ]
    model_path = next((c for c in candidates if os.path.exists(c)), None)
    if model_path:
        try:
            from piper.voice import PiperVoice
            piper_voice = PiperVoice.load(model_path)
            emit({"type": "log", "category": "TTS", "level": "INFO", "message": f"Piper neural voice loaded ({os.path.basename(model_path)})."})
            return
        except Exception as e:
            emit({"type": "log", "category": "TTS", "level": "WARN", "message": f"Piper init failed, falling back to pyttsx3: {e}"})

    try:
        import pyttsx3
        pyttsx_engine = pyttsx3.init()
        pyttsx_engine.setProperty('rate', 185)
        voices = pyttsx_engine.getProperty('voices')
        for v in voices:
            if 'zira' in v.name.lower():
                pyttsx_engine.setProperty('voice', v.id)
                break
        emit({"type": "log", "category": "TTS", "level": "INFO", "message": "pyttsx3 offline voice engine loaded."})
    except Exception as e:
        emit({"type": "log", "category": "TTS", "level": "ERROR", "message": f"pyttsx3 init failed: {e}"})

def speak_text(raw_text: str):
    global is_speaking
    is_speaking = True
    try:
        clean_text = clean_for_tts(raw_text)
        if not clean_text:
            return

        emit({"type": "state", "state": "speaking", "text": clean_text})

        played = False
        if piper_voice is not None:
            try:
                import io, wave
                buf = io.BytesIO()
                with wave.open(buf, 'wb') as wav_file:
                    piper_voice.synthesize_wav(clean_text, wav_file)
                buf.seek(0)
                with wave.open(buf, 'rb') as wav_file:
                    n_frames = wav_file.getnframes()
                    if n_frames > 0:
                        framerate = wav_file.getframerate()
                        frames = wav_file.readframes(n_frames)
                        audio_data = np.frombuffer(frames, dtype=np.int16)
                        if len(audio_data) > 0:
                            sd.play(audio_data, samplerate=framerate)
                            sd.wait()
                            played = True
            except Exception as e:
                emit({"type": "log", "category": "TTS", "level": "WARN", "message": f"Piper playback warning: {e}"})

        # Reliable fallback to pyttsx3 if piper produced 0 frames or failed
        if not played and pyttsx_engine is not None:
            try:
                pyttsx_engine.say(clean_text)
                pyttsx_engine.runAndWait()
                played = True
            except Exception as e:
                emit({"type": "log", "category": "TTS", "level": "ERROR", "message": f"pyttsx3 error: {e}"})

        # Small pause to prevent microphone pickup of speaker reverb
        time.sleep(0.2)
    except Exception as e:
        emit({"type": "log", "category": "TTS", "level": "ERROR", "message": f"TTS speak error: {e}"})
    finally:
        # Drain any residual audio frames accumulated during speech
        while not audio_queue.empty():
            try:
                audio_queue.get_nowait()
            except queue.Empty:
                break
        is_speaking = False
        emit({"type": "speech_done"})
        emit({"type": "state", "state": "listening", "text": ""})

# STT Whisper Model Initialization
whisper_model = None

def init_stt():
    global whisper_model
    try:
        from faster_whisper import WhisperModel
        import torch
        device = "cuda" if torch.cuda.is_available() else "cpu"
        compute_type = "float16" if device == "cuda" else "int8"
        cpu_threads = 4 if device == "cpu" else 1
        emit({"type": "log", "category": "STT", "level": "INFO", "message": f"Loading faster-whisper base.en on {device} ({compute_type}, {cpu_threads} threads)..."})
        whisper_model = WhisperModel("base.en", device=device, compute_type=compute_type, cpu_threads=cpu_threads)
        emit({"type": "log", "category": "STT", "level": "SUCCESS", "message": f"Whisper STT ready on {device}."})
    except Exception as e:
        emit({"type": "log", "category": "STT", "level": "ERROR", "message": f"Whisper STT init error: {e}"})

def audio_callback(indata, frames, time_info, status):
    """Callback from sounddevice audio stream."""
    if not is_speaking:
        audio_queue.put(indata[:, 0].copy())

def stdin_listener():
    """Reads JSON commands from backend on stdin without buffering delay."""
    global is_running
    try:
        if hasattr(sys.stdin, "reconfigure"):
            sys.stdin.reconfigure(line_buffering=True)
    except Exception:
        pass

    while is_running:
        try:
            line = sys.stdin.readline()
            if not line:
                break
            line = line.strip()
            if not line:
                continue
            cmd = json.loads(line)
            command_queue.put(cmd)
        except Exception as e:
            emit({"type": "log", "category": "SYSTEM", "level": "ERROR", "message": f"Invalid stdin JSON: {e}"})

def command_worker():
    """Processes incoming commands from Rust backend."""
    global is_running
    while is_running:
        try:
            cmd = command_queue.get(timeout=0.1)
            action = cmd.get("command")
            if action == "speak":
                text = cmd.get("text", "")
                speak_text(text)
            elif action == "stop":
                is_running = False
                break
        except queue.Empty:
            continue
        except Exception as e:
            emit({"type": "log", "category": "SYSTEM", "level": "ERROR", "message": f"Command worker error: {e}"})

# List of standalone filler words that should NOT trigger LLM processing
FILLER_WORDS = {
    "um", "uh", "ah", "oh", "yeah", "yes", "hmm", "right", "ok", "okay",
    "yep", "sure", "huh", "mhm", "aha", "so", "like"
}

def main():
    emit({"type": "log", "category": "SYSTEM", "level": "INFO", "message": "Initializing AURA Offline Speech Engine..."})
    
    init_tts()
    init_stt()

    if whisper_model is None:
        emit({"type": "log", "category": "STT", "level": "ERROR", "message": "Cannot start voice loop: Whisper model failed to load."})
        return

    # Start stdin reader thread
    t_stdin = threading.Thread(target=stdin_listener, daemon=True)
    t_stdin.start()

    # Start command worker thread
    t_cmd = threading.Thread(target=command_worker, daemon=True)
    t_cmd.start()

    # Signal ready to Rust backend
    emit({"type": "ready"})
    emit({"type": "state", "state": "listening", "text": ""})

    # Start sounddevice input stream
    try:
        stream = sd.InputStream(
            samplerate=SAMPLE_RATE,
            blocksize=BLOCK_SIZE,
            channels=CHANNELS,
            dtype='float32',
            callback=audio_callback
        )
    except Exception as e:
        emit({"type": "log", "category": "AUDIO", "level": "ERROR", "message": f"Cannot open microphone: {e}"})
        return

    pre_buffer_size = int((PRE_BUFFER_SEC * SAMPLE_RATE) / BLOCK_SIZE)
    silence_blocks_needed = int((SILENCE_DURATION_SEC * SAMPLE_RATE) / BLOCK_SIZE)
    min_speech_blocks = int((MIN_SPEECH_DURATION_SEC * SAMPLE_RATE) / BLOCK_SIZE)

    pre_buffer = []
    speech_frames = []
    in_speech = False
    silent_blocks_count = 0

    with stream:
        emit({"type": "log", "category": "SPEECH", "level": "INFO", "message": "🎙️ Always-on microphone listening stream active."})
        while is_running:
            try:
                chunk = audio_queue.get(timeout=0.2)
            except queue.Empty:
                continue

            if is_speaking:
                in_speech = False
                speech_frames.clear()
                pre_buffer.clear()
                silent_blocks_count = 0
                continue

            # Calculate RMS energy of current audio block
            rms = float(np.sqrt(np.mean(chunk**2)))

            if rms >= VAD_ENERGY_THRESHOLD:
                if not in_speech:
                    in_speech = True
                    emit({"type": "state", "state": "listening", "text": "Listening..."})
                    speech_frames = list(pre_buffer)
                speech_frames.append(chunk)
                silent_blocks_count = 0
            else:
                if in_speech:
                    speech_frames.append(chunk)
                    silent_blocks_count += 1
                    if silent_blocks_count >= silence_blocks_needed:
                        # User finished speaking natural sentence
                        in_speech = False
                        if len(speech_frames) >= min_speech_blocks:
                            emit({"type": "state", "state": "thinking", "text": "Processing..."})
                            full_audio = np.concatenate(speech_frames).astype(np.float32)
                            try:
                                segments, _ = whisper_model.transcribe(
                                    full_audio,
                                    beam_size=1,
                                    language="en",
                                    temperature=0.0,
                                    vad_filter=False
                                )
                                transcribed_text = " ".join([s.text for s in segments]).strip()
                                
                                # Filter hallucinations and standalone filler words
                                hallucinations = ["[music]", "(music)", "you", "thank you.", "bye.", "thank you for watching"]
                                words = [w.strip(".,!?:;\"'").lower() for w in transcribed_text.split()]

                                is_hallucination = transcribed_text.lower() in hallucinations
                                is_only_filler = len(words) == 1 and words[0] in FILLER_WORDS
                                is_empty = len(words) == 0

                                if not is_hallucination and not is_only_filler and not is_empty and len(transcribed_text) > 1:
                                    emit({
                                        "type": "transcription",
                                        "text": transcribed_text
                                    })
                                else:
                                    # Stay shut and reset to clean listening
                                    emit({"type": "state", "state": "listening", "text": ""})
                            except Exception as e:
                                emit({"type": "log", "category": "STT", "level": "ERROR", "message": f"Transcription error: {e}"})
                                emit({"type": "state", "state": "listening", "text": ""})
                        else:
                            emit({"type": "state", "state": "listening", "text": ""})

                        speech_frames.clear()
                        silent_blocks_count = 0
                else:
                    # Update rolling pre-buffer
                    pre_buffer.append(chunk)
                    if len(pre_buffer) > pre_buffer_size:
                        pre_buffer.pop(0)

if __name__ == "__main__":
    main()
