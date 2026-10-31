// AURA-1 Controller : Translucent Squinting Orb Interface with Live Dynamic Audio Reactivity & Dynamic Color Themes

const squintingOrb = document.getElementById('squintingOrb');
const speechPill = document.getElementById('speechPill');
const speechBadge = document.getElementById('speechBadge');
const speechText = document.getElementById('speechText');
const statusDot = document.getElementById('statusDot');
const orbContainer = document.getElementById('orbContainer');
const squintEyes = document.getElementById('squintEyes');
const agentMouth = document.getElementById('agentMouth');

let fadeTimer = null;

// Dynamic Theme Color Map
const COLOR_THEMES = {
  // Reference Default: Sky Blue to Purple / Violet Iridescent
  default: {
    top: '#a5f3fc',
    mid: '#60a5fa',
    base: '#a855f7',
    rimPrimary: '#38bdf8',
    rimSecondary: '#c084fc',
    glow: 'rgba(96, 165, 250, 0.55)',
    ambient: 'rgba(168, 85, 247, 0.35)',
    eyeAmbient: 'rgba(232, 121, 249, 0.85)',
    accentDot: '#38bdf8',
    pillBorder: 'rgba(96, 165, 250, 0.3)'
  },
  red: {
    top: '#fecdd3',
    mid: '#f43f5e',
    base: '#be123c',
    rimPrimary: '#fb7185',
    rimSecondary: '#e11d48',
    glow: 'rgba(244, 63, 94, 0.65)',
    ambient: 'rgba(190, 18, 60, 0.45)',
    eyeAmbient: 'rgba(251, 113, 133, 0.9)',
    accentDot: '#fb7185',
    pillBorder: 'rgba(244, 63, 94, 0.4)'
  },
  blue: {
    top: '#bae6fd',
    mid: '#0284c7',
    base: '#1e40af',
    rimPrimary: '#38bdf8',
    rimSecondary: '#60a5fa',
    glow: 'rgba(56, 189, 248, 0.65)',
    ambient: 'rgba(30, 64, 175, 0.4)',
    eyeAmbient: 'rgba(56, 189, 248, 0.85)',
    accentDot: '#38bdf8',
    pillBorder: 'rgba(56, 189, 248, 0.4)'
  },
  purple: {
    top: '#e9d5ff',
    mid: '#a855f7',
    base: '#6b21a8',
    rimPrimary: '#c084fc',
    rimSecondary: '#9333ea',
    glow: 'rgba(168, 85, 247, 0.65)',
    ambient: 'rgba(107, 33, 168, 0.45)',
    eyeAmbient: 'rgba(216, 180, 254, 0.9)',
    accentDot: '#c084fc',
    pillBorder: 'rgba(168, 85, 247, 0.4)'
  },
  violet: {
    top: '#ddd6fe',
    mid: '#8b5cf6',
    base: '#5b21b6',
    rimPrimary: '#a78bfa',
    rimSecondary: '#7c3aed',
    glow: 'rgba(139, 92, 246, 0.65)',
    ambient: 'rgba(91, 33, 182, 0.45)',
    eyeAmbient: 'rgba(167, 139, 250, 0.9)',
    accentDot: '#a78bfa',
    pillBorder: 'rgba(139, 92, 246, 0.4)'
  },
  green: {
    top: '#a7f3d0',
    mid: '#10b981',
    base: '#047857',
    rimPrimary: '#34d399',
    rimSecondary: '#059669',
    glow: 'rgba(16, 185, 129, 0.65)',
    ambient: 'rgba(4, 120, 87, 0.45)',
    eyeAmbient: 'rgba(110, 231, 183, 0.85)',
    accentDot: '#34d399',
    pillBorder: 'rgba(16, 185, 129, 0.4)'
  },
  emerald: {
    top: '#6ee7b7',
    mid: '#059669',
    base: '#064e3b',
    rimPrimary: '#10b981',
    rimSecondary: '#047857',
    glow: 'rgba(5, 150, 105, 0.65)',
    ambient: 'rgba(6, 78, 59, 0.5)',
    eyeAmbient: 'rgba(52, 211, 153, 0.85)',
    accentDot: '#10b981',
    pillBorder: 'rgba(5, 150, 105, 0.4)'
  },
  gold: {
    top: '#fef08a',
    mid: '#eab308',
    base: '#a16207',
    rimPrimary: '#facc15',
    rimSecondary: '#ca8a04',
    glow: 'rgba(234, 179, 8, 0.65)',
    ambient: 'rgba(161, 98, 7, 0.45)',
    eyeAmbient: 'rgba(254, 240, 138, 0.9)',
    accentDot: '#facc15',
    pillBorder: 'rgba(234, 179, 8, 0.4)'
  },
  yellow: {
    top: '#fef9c3',
    mid: '#facc15',
    base: '#ca8a04',
    rimPrimary: '#fde047',
    rimSecondary: '#eab308',
    glow: 'rgba(250, 204, 21, 0.65)',
    ambient: 'rgba(202, 138, 4, 0.45)',
    eyeAmbient: 'rgba(254, 240, 138, 0.9)',
    accentDot: '#fde047',
    pillBorder: 'rgba(250, 204, 21, 0.4)'
  },
  orange: {
    top: '#fed7aa',
    mid: '#f97316',
    base: '#c2410c',
    rimPrimary: '#fb923c',
    rimSecondary: '#ea580c',
    glow: 'rgba(249, 115, 22, 0.65)',
    ambient: 'rgba(194, 65, 12, 0.45)',
    eyeAmbient: 'rgba(253, 186, 116, 0.85)',
    accentDot: '#fb923c',
    pillBorder: 'rgba(249, 115, 22, 0.4)'
  },
  pink: {
    top: '#fbcfe8',
    mid: '#ec4899',
    base: '#be185d',
    rimPrimary: '#f472b6',
    rimSecondary: '#db2777',
    glow: 'rgba(236, 72, 153, 0.65)',
    ambient: 'rgba(190, 24, 93, 0.45)',
    eyeAmbient: 'rgba(244, 114, 182, 0.9)',
    accentDot: '#f472b6',
    pillBorder: 'rgba(236, 72, 153, 0.4)'
  },
  cyan: {
    top: '#cffafe',
    mid: '#06b6d4',
    base: '#0e7490',
    rimPrimary: '#22d3ee',
    rimSecondary: '#0891b2',
    glow: 'rgba(6, 182, 212, 0.65)',
    ambient: 'rgba(14, 116, 144, 0.45)',
    eyeAmbient: 'rgba(103, 232, 249, 0.9)',
    accentDot: '#22d3ee',
    pillBorder: 'rgba(6, 182, 212, 0.4)'
  },
  white: {
    top: '#ffffff',
    mid: '#e2e8f0',
    base: '#94a3b8',
    rimPrimary: '#ffffff',
    rimSecondary: '#cbd5e1',
    glow: 'rgba(255, 255, 255, 0.7)',
    ambient: 'rgba(148, 163, 184, 0.4)',
    eyeAmbient: 'rgba(255, 255, 255, 0.95)',
    accentDot: '#ffffff',
    pillBorder: 'rgba(255, 255, 255, 0.4)'
  }
};

/**
 * Apply Dynamic Color Palette to the UI
 */
function applyColorTheme(colorName) {
  if (!colorName) return;
  const name = colorName.trim().toLowerCase();
  const theme = COLOR_THEMES[name] || COLOR_THEMES['default'];
  const root = document.documentElement;

  root.style.setProperty('--orb-gradient-top', theme.top);
  root.style.setProperty('--orb-gradient-mid', theme.mid);
  root.style.setProperty('--orb-gradient-base', theme.base);
  root.style.setProperty('--orb-rim-primary', theme.rimPrimary);
  root.style.setProperty('--orb-rim-secondary', theme.rimSecondary);
  root.style.setProperty('--orb-glow', theme.glow);
  root.style.setProperty('--orb-ambient', theme.ambient);
  root.style.setProperty('--eye-ambient-glow', theme.eyeAmbient);
  root.style.setProperty('--accent-dot', theme.accentDot);
  root.style.setProperty('--pill-border', theme.pillBorder);

  // Subtle flash on color change
  if (squintingOrb) {
    squintingOrb.style.filter = 'brightness(1.35)';
    setTimeout(() => {
      squintingOrb.style.filter = '';
    }, 350);
  }
}

// Helper to set orb state and speech bubble content
function setOrbState(state, text = '', badge = '') {
  if (!squintingOrb || !speechPill) return;

  squintingOrb.dataset.state = state;

  if (fadeTimer) {
    clearTimeout(fadeTimer);
    fadeTimer = null;
  }

  // Handle Badge label
  if (badge) {
    speechBadge.textContent = badge;
  } else {
    switch (state) {
      case 'listening':
        speechBadge.textContent = 'LISTENING';
        break;
      case 'thinking':
        speechBadge.textContent = 'PROCESSING';
        break;
      case 'speaking':
        speechBadge.textContent = 'AURA-1';
        break;
      default:
        speechBadge.textContent = 'AURA-1';
        break;
    }
  }

  // Handle Text and Visibility
  if (text && text.trim().length > 0) {
    speechText.textContent = text;
    speechPill.classList.add('visible');
  } else {
    // Empty text: if listening or idle, smoothly fade away after brief moment
    if (state === 'idle' || state === 'listening') {
      fadeTimer = setTimeout(() => {
        speechPill.classList.remove('visible');
        speechText.textContent = '';
      }, 2000);
    }
  }

  // When speaking completes or returns to listening, fade away after 2.5 seconds
  if (state === 'listening' && text === 'Listening...') {
    fadeTimer = setTimeout(() => {
      speechPill.classList.remove('visible');
      speechText.textContent = '';
    }, 2500);
  }
}

// Window Dragging Support
if (orbContainer) {
  orbContainer.addEventListener('mousedown', (e) => {
    if (e.button === 0) {
      try {
        if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
          window.__TAURI__.core.invoke('drag_window');
        } else if (window.__TAURI__ && window.__TAURI__.invoke) {
          window.__TAURI__.invoke('drag_window');
        }
      } catch (err) {
        console.warn('Drag invoke error:', err);
      }
    }
  });
}

// Click or Keyboard on Orb gives playful squint and smile bounce
if (squintingOrb) {
  const triggerSmileBounce = () => {
    squintingOrb.style.transform = 'scale(0.9)';
    if (squintEyes) squintEyes.style.transform = 'scaleX(1.3) scaleY(0.4)';
    const mouthShape = agentMouth ? agentMouth.querySelector('.mouth-shape') : null;
    if (mouthShape) mouthShape.style.transform = 'scale(1.4)';
    setTimeout(() => {
      squintingOrb.style.transform = '';
      if (squintEyes) squintEyes.style.transform = '';
      if (mouthShape) mouthShape.style.transform = '';
    }, 220);
  };

  squintingOrb.addEventListener('click', triggerSmileBounce);

  squintingOrb.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      triggerSmileBounce();
    }
  });
}

// Connect to Tauri Live Events
function initTauriListeners() {
  const tauri = window.__TAURI__;
  if (!tauri) {
    return;
  }

  const listen = (tauri.event && tauri.event.listen) || (tauri.listen);
  if (!listen) {
    return;
  }

  // 1. Listen for AURA state transitions ("idle", "listening", "thinking", "speaking")
  listen('aura-state', (event) => {
    const payload = event.payload || {};
    const state = payload.state || 'idle';
    const text = payload.text || '';
    setOrbState(state, text);
  });

  // 2. Listen for transcribed user speech
  listen('aura-user-speech', (event) => {
    const payload = event.payload || {};
    const text = payload.text || '';
    if (text) {
      setOrbState('thinking', `"${text}"`, 'YOU');
    }
  });

  // 3. Listen for AURA agent spoken responses
  listen('aura-agent-speech', (event) => {
    const payload = event.payload || {};
    const text = payload.text || '';
    if (text) {
      setOrbState('speaking', text, 'AURA-1');
    }
  });

  // 4. Listen for dynamic color changes requested by voice
  listen('aura-color', (event) => {
    const payload = event.payload || {};
    const color = payload.color;
    if (color) {
      applyColorTheme(color);
    }
  });

  // 5. Listen for Vision Hand Tracking status
  listen('aura-hand-tracking-status', (event) => {
    const payload = event.payload || {};
    if (payload.active) {
      setOrbState('thinking', 'Vision: Tracking Hand...', 'VISION');
      if (squintingOrb) squintingOrb.classList.add('tracking-active');
    } else {
      if (squintingOrb) {
        squintingOrb.classList.remove('tracking-active');
        squintingOrb.classList.remove('tracking-detected');
      }
      setOrbState('listening', '', 'IDLE');
    }
  });

  // 6. Listen for real-time hand coordinates from webcam
  listen('aura-hand-position', (event) => {
    const payload = event.payload || {};
    if (payload.detected) {
      if (squintingOrb) squintingOrb.classList.add('tracking-detected');
    } else {
      if (squintingOrb) squintingOrb.classList.remove('tracking-detected');
    }
  });

  // 7. Listen for real-time gesture triggers (swipe_right, swipe_left, swipe_up, swipe_down, pinch, peace, thumbs_up, open_palm)
  listen('aura-gesture', (event) => {
    const payload = event.payload || {};
    const gesture = payload.gesture || '';
    if (!gesture) return;

    const gestureLabels = {
      swipe_right: '👉 Swiped Right',
      swipe_left: '👈 Swiped Left',
      swipe_up: '👆 Swiped Up',
      swipe_down: '👇 Swiped Down',
      pinch: '🤏 Pinched (Minimize)',
      peace: '✌️ Peace Sign',
      thumbs_up: '👍 Thumbs Up!',
      open_palm: '✋ Open Palm (Center)',
      fist: '✊ Fist (Halt)'
    };

    const label = gestureLabels[gesture] || `Gesture: ${gesture}`;
    setOrbState('thinking', label, 'GESTURE');

    if (squintingOrb) {
      squintingOrb.dataset.gesture = gesture;
      squintingOrb.classList.add('gesture-triggered');
      setTimeout(() => {
        squintingOrb.classList.remove('gesture-triggered');
        delete squintingOrb.dataset.gesture;
      }, 750);
    }
  });
}

// Initialize on page load
window.addEventListener('DOMContentLoaded', () => {
  if (squintingOrb) squintingOrb.dataset.state = 'listening';
  initTauriListeners();
});
