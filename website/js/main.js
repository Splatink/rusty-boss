/* Rusty Boss companion page — theme switch, oscilloscope, knobs, tap tempo, drag lab. */
(() => {
  "use strict";
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  const getVar = (name) =>
    getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  // ===== Theme switching =====
  const themeBtns = document.querySelectorAll("[data-theme]");
  const applyTheme = (t) => {
    document.documentElement.dataset.theme = t;
    themeBtns.forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.theme === t)));
    document.querySelectorAll(".theme-card").forEach((c) =>
      c.classList.toggle("active", c.dataset.theme === t));
    try { localStorage.setItem("rusty-boss-theme", t); } catch { /* private mode */ }
  };
  themeBtns.forEach((b) => b.addEventListener("click", () => applyTheme(b.dataset.theme)));
  try {
    const saved = localStorage.getItem("rusty-boss-theme");
    if (saved && ["puppet", "justice", "kill", "lightning"].includes(saved)) applyTheme(saved);
  } catch { /* ignore */ }

  // ===== Knobs =====
  const MIN_ANGLE = -135, MAX_ANGLE = 135;
  const knobs = [...document.querySelectorAll("[data-knob]")];
  const setKnob = (knob, value, snap = false) => {
    let v = snap ? Math.round(value) : value;
    v = Math.max(0, Math.min(100, v));
    knob.dataset.value = v;
    knob.setAttribute("aria-valuenow", String(Math.round(v)));
    const angle = MIN_ANGLE + (v / 100) * (MAX_ANGLE - MIN_ANGLE);
    knob.querySelector(".knob-cap").style.transform = `rotate(${angle}deg)`;
    const read = knob.parentElement.querySelector(".knob-val");
    if (read) read.textContent = String(Math.round(v));
  };

  knobs.forEach((knob) => {
    setKnob(knob, Number(knob.getAttribute("aria-valuenow")) || 0);
    let startY = 0, startVal = 0, dragging = false;
    knob.addEventListener("pointerdown", (e) => {
      dragging = true; startY = e.clientY; startVal = Number(knob.dataset.value);
      knob.setPointerCapture(e.pointerId);
      knob.querySelector(".knob-cap").style.transition = "none";
    });
    knob.addEventListener("pointermove", (e) => {
      if (!dragging) return;
      setKnob(knob, startVal + (startY - e.clientY) * 0.7);
    });
    const finish = () => {
      if (!dragging) return;
      dragging = false;
      knob.querySelector(".knob-cap").style.transition = "";
      setKnob(knob, Number(knob.dataset.value), true);
    };
    knob.addEventListener("pointerup", finish);
    knob.addEventListener("pointercancel", finish);
    knob.addEventListener("keydown", (e) => {
      const v = Number(knob.dataset.value);
      const step = { ArrowUp: 1, ArrowDown: -1, PageUp: 10, PageDown: -10, Home: 100, End: 0 };
      if (e.key in step) { setKnob(knob, v + step[e.key], true); e.preventDefault(); }
    });
    knob.addEventListener("dblclick", () => {
      const read = knob.parentElement.querySelector(".knob-val");
      if (!read) return;
      const val = prompt("Exact value (0–100):", read.textContent);
      if (val === null) return;
      const n = Number(val);
      if (!Number.isFinite(n)) return;
      setKnob(knob, n, true);
    });
  });

  // ===== Hero oscilloscope =====
  const scope = document.getElementById("scope");
  const scopeKnob = scope.closest(".scope-wrap")?.querySelector("[data-knob]");
  if (scope && scopeKnob) {
    const ctx = scope.getContext("2d");
    let phase = 0;
    const draw = () => {
      const w = scope.width, h = scope.height;
      const accent = getVar("--accent") || "#f4a65e";
      const raised = getVar("--raised") || "#3a2d24";
      const border = getVar("--border") || "#554233";
      const level = (Number(scopeKnob.dataset.value) || 0) / 100;
      const amp = (h / 2 - 12) * (0.15 + 0.85 * level);
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = raised;
      ctx.fillRect(0, 0, w, h);
      // grid
      ctx.strokeStyle = border;
      ctx.globalAlpha = 0.35;
      ctx.lineWidth = 1;
      for (let x = 0; x <= w; x += w / 16) { ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke(); }
      ctx.beginPath(); ctx.moveTo(0, h / 2); ctx.lineTo(w, h / 2); ctx.stroke();
      ctx.globalAlpha = 1;
      // waveform with a touch of drive
      ctx.strokeStyle = accent;
      ctx.lineWidth = 2;
      ctx.shadowColor = accent;
      ctx.shadowBlur = reduced ? 0 : 8;
      ctx.beginPath();
      for (let x = 0; x <= w; x++) {
        const t = (x / w) * Math.PI * 6;
        const y = h / 2 - Math.sin(t + phase) * amp * (1 - 0.12 * Math.sin(3 * (t + phase)) * level);
        if (x === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
      ctx.stroke();
      ctx.shadowBlur = 0;
    };
    if (reduced) { draw(); }
    else {
      const loop = () => {
        phase += 0.16;
        draw();
        requestAnimationFrame(loop);
      };
      requestAnimationFrame(loop);
    }
  }

  // ===== Scroll reveals =====
  const io = new IntersectionObserver((entries) => {
    for (const en of entries) {
      if (en.isIntersecting) {
        const el = en.target;
        if (el.hasAttribute("data-step")) el.style.setProperty("--n", el.dataset.step);
        el.classList.add("in");
        io.unobserve(el);
      }
    }
  }, { threshold: 0.15 });
  document.querySelectorAll("[data-reveal], [data-step]").forEach((el) => io.observe(el));
  document.querySelectorAll("[data-step]").forEach((el, i) => el.dataset.step = i + 1);

  // ===== Protection modes =====
  const MODE_TEXT = {
    readonly: {
      title: "Read-only",
      body: "Browse tones locally and read the amp. Clicking a tone does not change the amp's sound. Preview, live knobs, channel switching and saved-channel writes are blocked. Connecting still sends the session setup/cleanup commands the connection needs — it never touches tone parameters.",
    },
    preview: {
      title: "Preview & edit",
      body: "Click a tone to hear it and adjust the current sound; amp knobs and effects become live after the preview is read back. Saved channels remain protected — your saved channels are unchanged. If PANEL is active, the app first switches to A1 so PANEL is not used as a scratch buffer.",
    },
    writes: {
      title: "Amp writes enabled",
      body: "Allows confirmed channel saves, system-setting Apply and full restore. Drag a tone onto an amp channel and answer Yes to the overwrite confirmation — saving replaces only that channel's previous tone. Every destructive operation still asks first, and clicking Lock ends the session.",
    },
  };
  const modeCards = document.querySelectorAll(".mode-card");
  const modeTitle = document.getElementById("mode-title");
  const modeText = document.getElementById("mode-text");
  const detail = document.getElementById("mode-detail");
  modeCards.forEach((card) => {
    card.addEventListener("click", () => {
      modeCards.forEach((c) => c.setAttribute("aria-pressed", "false"));
      card.setAttribute("aria-pressed", "true");
      const info = MODE_TEXT[card.dataset.mode];
      modeTitle.textContent = info.title;
      modeText.textContent = info.body;
      detail.style.animation = "none";
      void detail.offsetWidth; // retrigger the entry animation
      detail.style.animation = "glow-in .4s ease";
    });
  });

  // ===== Tap tempo =====
  const pad = document.getElementById("tap-pad");
  const bpmRead = document.getElementById("bpm-read");
  const pulse = document.getElementById("pulse");
  let taps = [];
  pad.addEventListener("click", () => {
    taps.push(performance.now());
    taps = taps.slice(-4);
    const iv = (taps[taps.length - 1] - taps[0]) / (taps.length - 1);
    const bpm = iv > 0 ? Math.round(60000 / iv) : null;
    bpmRead.textContent = bpm ? `${bpm} BPM` : "tap more";
    if (!reduced && bpm) pulse.style.animation = `pulse-beat ${60 / bpm}s linear infinite`;
  });

  // ===== Drag lab =====
  const chips = document.querySelectorAll(".chip");
  const chans = document.querySelectorAll(".chan");
  const toast = document.getElementById("toast");
  const say = (msg) => {
    toast.textContent = msg;
    toast.style.animation = "none";
    void toast.offsetWidth;
    toast.style.animation = "glow-in .35s ease";
  };
  chips.forEach((chip) => {
    chip.addEventListener("dragstart", (e) => {
      e.dataTransfer.setData("text/plain", chip.dataset.tone);
      e.dataTransfer.effectAllowed = "move";
      chip.classList.add("dragging");
    });
    chip.addEventListener("dragend", () => chip.classList.remove("dragging"));
  });
  chans.forEach((chan) => {
    chan.addEventListener("dragover", (e) => { e.preventDefault(); chan.classList.add("dragover"); });
    chan.addEventListener("dragleave", () => chan.classList.remove("dragover"));
    chan.addEventListener("drop", (e) => {
      e.preventDefault();
      chan.classList.remove("dragover");
      const tone = e.dataTransfer.getData("text/plain") || "tone";
      chan.classList.remove("filled", "dropped");
      void chan.offsetWidth;
      chan.classList.add("filled", "dropped");
      chan.querySelector(".chan-name").textContent = `${tone}`;
      say(`Saved ${tone} → ${chan.id.replace("chan-", "")}. Overwrite confirmed — previous channel tone replaced.`);
    });
  });
  document.getElementById("reset-lab").addEventListener("click", () => {
    chans.forEach((c) => {
      c.classList.remove("filled", "dropped", "dragover");
      c.querySelector(".chan-name").textContent = c.id.replace("chan-", "");
    });
    say("Lab reset. Nothing was written to any real amplifier.");
  });

  // ===== Copy build commands =====
  const copyBtn = document.getElementById("copy-build");
  if (copyBtn) {
    copyBtn.addEventListener("click", () => {
      const code = document.getElementById("build-code").textContent.trim();
      navigator.clipboard.write(code).then(
        () => { copyBtn.textContent = "Copied"; setTimeout(() => (copyBtn.textContent = "Copy"), 1500); },
        () => { copyBtn.textContent = "Failed"; setTimeout(() => (copyBtn.textContent = "Copy"), 1500); }
      );
    });
  }
})();
