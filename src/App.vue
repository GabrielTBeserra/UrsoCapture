<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";

// ==================== INTERFACES ====================
interface MediaDeviceInfoItem {
  deviceId: string;
  label: string;
  kind: MediaDeviceKind;
  groupId: string;
}

interface ProcessStats {
  ram_working_set_bytes: number;
  ram_working_set_mb: number;
  ram_peak_mb: number;
  pid: number;
  platform: string;
}

// Global Refs
const videoElementRef = ref<HTMLVideoElement | null>(null);
const videoContainerRef = ref<HTMLDivElement | null>(null);
const audioVisualizerCanvasRef = ref<HTMLCanvasElement | null>(null);

// Device Lists
const videoDevices = ref<MediaDeviceInfoItem[]>([]);
const audioDevices = ref<MediaDeviceInfoItem[]>([]);

// Selected Device IDs
const selectedVideoId = ref<string>("");
const selectedAudioId = ref<string>("");

// Video Display & Anti-Border Calibration
type FitMode = "cover" | "contain" | "fill";
const fitMode = ref<FitMode>("cover"); // Default 'cover' eliminates letterbox/white bars
const videoZoom = ref<number>(100); // 100% to 120% for capture card overscan/crop
const brightness = ref<number>(100); // 50% to 150%
const contrast = ref<number>(100); // 50% to 150%
const saturation = ref<number>(100); // 50% to 150%
const mirrorVideo = ref<boolean>(false);
const showGrid = ref<boolean>(false);

// Stream Quality Settings
const targetResolution = ref<string>("1080p");
const targetFps = ref<number>(60);
const rawStudioAudio = ref<boolean>(true); // 0 processing delay, 48kHz studio audio

// Audio Monitoring & Volume
const isAudioMonitoringActive = ref<boolean>(true); // Active by default for direct game audio!
const monitorVolume = ref<number>(90);
const audioPeakLevel = ref<number>(0);
const audioDbLevel = ref<number>(-60);
const isAudioClipping = ref<boolean>(false);

// Telemetry & State
const isStreamActive = ref<boolean>(false);
const streamError = ref<string | null>(null);
const liveVideoWidth = ref<number>(0);
const liveVideoHeight = ref<number>(0);
const liveFps = ref<number>(0);
const processStats = ref<ProcessStats | null>(null);

// UI State
const isSidebarOpen = ref<boolean>(false);
const isFullscreen = ref<boolean>(false);
const showControlsOverlay = ref<boolean>(true);
const toastMessage = ref<string | null>(null);

// Recording Test
const isRecordingTest = ref<boolean>(false);
const recordedVideoUrl = ref<string | null>(null);
let mediaRecorder: MediaRecorder | null = null;
let recordedBlobs: Blob[] = [];

// Audio Graph & Timers
let activeStream: MediaStream | null = null;
let audioContext: AudioContext | null = null;
let audioSourceNode: MediaStreamAudioSourceNode | null = null;
let analyserNode: AnalyserNode | null = null;
let monitorGainNode: GainNode | null = null;
let animFrameId: number | null = null;
let fpsIntervalTimer: any = null;
let statsTimer: any = null;
let hideControlsTimer: any = null;
let frameCount = 0;
let lastFpsCheck = performance.now();

// Toast helper
function showToast(msg: string) {
  toastMessage.value = msg;
  setTimeout(() => {
    if (toastMessage.value === msg) toastMessage.value = null;
  }, 2500);
}

// ----------------- DEVICE ENUMERATION -----------------
async function refreshDevices() {
  try {
    try {
      const tempStream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
      tempStream.getTracks().forEach((t) => t.stop());
    } catch (_) {
      // Ignore if user dismisses initial prompt
    }

    const devices = await navigator.mediaDevices.enumerateDevices();

    videoDevices.value = devices
      .filter((d) => d.kind === "videoinput")
      .map((d, idx) => ({
        deviceId: d.deviceId,
        label: d.label || `Câmera / Placa de Captura ${idx + 1}`,
        kind: d.kind,
        groupId: d.groupId,
      }));

    audioDevices.value = devices
      .filter((d) => d.kind === "audioinput")
      .map((d, idx) => ({
        deviceId: d.deviceId,
        label: d.label || `Microfone / Entrada de Linha ${idx + 1}`,
        kind: d.kind,
        groupId: d.groupId,
      }));

    if (!selectedVideoId.value && videoDevices.value.length > 0) {
      selectedVideoId.value = videoDevices.value[0].deviceId;
    }
    if (!selectedAudioId.value && audioDevices.value.length > 0) {
      selectedAudioId.value = audioDevices.value[0].deviceId;
    }
  } catch (err: any) {
    console.error("Erro ao enumerar dispositivos:", err);
    streamError.value = "Não foi possível listar os dispositivos de mídia.";
  }
}

// ----------------- STREAM START / STOP (ZERO DELAY) -----------------
async function startLivePreview() {
  streamError.value = null;
  stopLivePreview();

  let resWidth = 1920;
  let resHeight = 1080;
  if (targetResolution.value === "4k") {
    resWidth = 3840;
    resHeight = 2160;
  } else if (targetResolution.value === "720p") {
    resWidth = 1280;
    resHeight = 720;
  } else if (targetResolution.value === "max") {
    resWidth = 4096;
    resHeight = 2160;
  }

  const videoConstraints: MediaTrackConstraints = {
    deviceId: selectedVideoId.value ? { exact: selectedVideoId.value } : undefined,
    width: { ideal: resWidth },
    height: { ideal: resHeight },
    frameRate: { ideal: targetFps.value, min: 24 },
    aspectRatio: { ideal: 1.7777777778 },
  };

  const audioConstraints: MediaTrackConstraints = {
    deviceId: selectedAudioId.value ? { exact: selectedAudioId.value } : undefined,
    echoCancellation: !rawStudioAudio.value,
    noiseSuppression: !rawStudioAudio.value,
    autoGainControl: !rawStudioAudio.value,
    channelCount: { ideal: 2 },
    sampleRate: { ideal: 48000 },
    sampleSize: { ideal: 16 },
  };

  try {
    const stream = await navigator.mediaDevices.getUserMedia({
      video: videoDevices.value.length > 0 ? videoConstraints : false,
      audio: audioDevices.value.length > 0 ? audioConstraints : false,
    });

    activeStream = stream;
    isStreamActive.value = true;

    await nextTick();

    if (videoElementRef.value) {
      videoElementRef.value.srcObject = stream;
      videoElementRef.value.play().catch(console.error);

      videoElementRef.value.onloadedmetadata = () => {
        if (videoElementRef.value) {
          liveVideoWidth.value = videoElementRef.value.videoWidth;
          liveVideoHeight.value = videoElementRef.value.videoHeight;
        }
      };
    }

    setupAudioAnalysis(stream);
    setupFpsMeter();
    showToast("Transmissão A/V ativa com 0 delay!");
  } catch (err: any) {
    console.error("Erro ao iniciar captura:", err);
    streamError.value = `Erro: ${err.message || "Falha ao abrir dispositivo"}`;
    isStreamActive.value = false;
  }
}

function stopLivePreview() {
  if (activeStream) {
    activeStream.getTracks().forEach((t) => t.stop());
    activeStream = null;
  }
  if (videoElementRef.value) {
    videoElementRef.value.srcObject = null;
  }
  if (audioContext) {
    audioContext.close().catch(() => {});
    audioContext = null;
  }
  if (animFrameId) {
    cancelAnimationFrame(animFrameId);
    animFrameId = null;
  }
  if (fpsIntervalTimer) {
    clearInterval(fpsIntervalTimer);
    fpsIntervalTimer = null;
  }
  isStreamActive.value = false;
  liveFps.value = 0;
  audioPeakLevel.value = 0;
  audioDbLevel.value = -60;
}

// ----------------- WEB AUDIO API (VU METER & MONITORING) -----------------
function setupAudioAnalysis(stream: MediaStream) {
  const audioTracks = stream.getAudioTracks();
  if (audioTracks.length === 0) return;

  try {
    const AudioCtxClass = window.AudioContext || (window as any).webkitAudioContext;
    audioContext = new AudioCtxClass({
      latencyHint: "interactive",
      sampleRate: 48000,
    });

    audioSourceNode = audioContext.createMediaStreamSource(stream);
    analyserNode = audioContext.createAnalyser();
    analyserNode.fftSize = 128;
    analyserNode.smoothingTimeConstant = 0.5;

    monitorGainNode = audioContext.createGain();
    monitorGainNode.gain.value = isAudioMonitoringActive.value ? monitorVolume.value / 100 : 0;

    audioSourceNode.connect(analyserNode);
    audioSourceNode.connect(monitorGainNode);
    monitorGainNode.connect(audioContext.destination);

    drawAudioVisualizer();
  } catch (err) {
    console.error("Erro configurando Web Audio:", err);
  }
}

function updateMonitoringVolume() {
  if (monitorGainNode && audioContext) {
    if (audioContext.state === "suspended") {
      audioContext.resume().catch(console.error);
    }
    monitorGainNode.gain.value = isAudioMonitoringActive.value ? monitorVolume.value / 100 : 0;
  }
}

function toggleAudioMute() {
  isAudioMonitoringActive.value = !isAudioMonitoringActive.value;
  updateMonitoringVolume();
  showToast(isAudioMonitoringActive.value ? "Áudio do jogo Ativado" : "Áudio do jogo Silenciado");
}

function drawAudioVisualizer() {
  if (!analyserNode || !isStreamActive.value) return;

  const bufferLength = analyserNode.frequencyBinCount;
  const timeData = new Uint8Array(bufferLength);
  const freqData = new Uint8Array(bufferLength);

  const canvas = audioVisualizerCanvasRef.value;
  const ctx = canvas ? canvas.getContext("2d") : null;

  function renderLoop() {
    if (!analyserNode || !isStreamActive.value) return;

    analyserNode.getByteTimeDomainData(timeData);
    analyserNode.getByteFrequencyData(freqData);

    let sumSquares = 0;
    let peak = 0;
    for (let i = 0; i < bufferLength; i++) {
      const norm = (timeData[i] - 128) / 128;
      sumSquares += norm * norm;
      const absVal = Math.abs(norm);
      if (absVal > peak) peak = absVal;
    }

    const rms = Math.sqrt(sumSquares / bufferLength);
    const db = rms > 0 ? 20 * Math.log10(rms) : -60;
    audioDbLevel.value = Math.max(-60, Math.min(0, Math.round(db)));
    audioPeakLevel.value = Math.min(100, Math.round(peak * 100));
    isAudioClipping.value = peak >= 0.98;

    if (ctx && canvas) {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      const barWidth = (canvas.width / (bufferLength / 2)) * 1.5;
      let barX = 0;
      for (let i = 0; i < bufferLength / 2; i++) {
        const barHeight = (freqData[i] / 255) * canvas.height;
        const grad = ctx.createLinearGradient(0, canvas.height, 0, 0);
        grad.addColorStop(0, "rgba(0, 242, 254, 0.4)");
        grad.addColorStop(0.7, "rgba(16, 185, 129, 0.9)");
        grad.addColorStop(1, "rgba(239, 68, 68, 1)");

        ctx.fillStyle = grad;
        ctx.fillRect(barX, canvas.height - barHeight, barWidth - 1, barHeight);
        barX += barWidth;
      }
    }

    animFrameId = requestAnimationFrame(renderLoop);
  }

  renderLoop();
}

// ----------------- FPS METER -----------------
function setupFpsMeter() {
  frameCount = 0;
  lastFpsCheck = performance.now();

  function countFrame() {
    frameCount++;
    if (videoElementRef.value && "requestVideoFrameCallback" in videoElementRef.value) {
      (videoElementRef.value as any).requestVideoFrameCallback(countFrame);
    }
  }

  if (videoElementRef.value && "requestVideoFrameCallback" in videoElementRef.value) {
    (videoElementRef.value as any).requestVideoFrameCallback(countFrame);
  }

  fpsIntervalTimer = setInterval(() => {
    const now = performance.now();
    const elapsed = (now - lastFpsCheck) / 1000;
    liveFps.value = Math.round(frameCount / elapsed);
    frameCount = 0;
    lastFpsCheck = now;
  }, 1000);
}

// ----------------- FULLSCREEN & AUTO-HIDE CONTROLS -----------------
async function toggleFullscreen() {
  try {
    if (!document.fullscreenElement) {
      await document.documentElement.requestFullscreen();
      isFullscreen.value = true;
      resetHideTimer();
    } else {
      await document.exitFullscreen();
      isFullscreen.value = false;
      showControlsOverlay.value = true;
    }
  } catch (err) {
    console.error("Erro ao alternar tela cheia:", err);
  }
}

function resetHideTimer() {
  showControlsOverlay.value = true;
  if (hideControlsTimer) clearTimeout(hideControlsTimer);

  // When in fullscreen and sidebar is closed, auto-hide UI & cursor after 2.5 seconds of inactivity
  if (isFullscreen.value && !isSidebarOpen.value) {
    hideControlsTimer = setTimeout(() => {
      showControlsOverlay.value = false;
    }, 2500);
  }
}

// ----------------- SNAPSHOT & TEST RECORDING -----------------
function takeSnapshot() {
  if (!videoElementRef.value) return;
  const video = videoElementRef.value;
  const canvas = document.createElement("canvas");
  canvas.width = video.videoWidth || 1920;
  canvas.height = video.videoHeight || 1080;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  if (mirrorVideo.value) {
    ctx.translate(canvas.width, 0);
    ctx.scale(-1, 1);
  }
  ctx.drawImage(video, 0, 0, canvas.width, canvas.height);

  const dataUrl = canvas.toDataURL("image/png");
  const a = document.createElement("a");
  a.href = dataUrl;
  a.download = `MatheusApp_Capture_${Date.now()}.png`;
  a.click();
  showToast("Foto HD salva em resolução total!");
}

async function startSyncTestRecording() {
  if (!activeStream || isRecordingTest.value) return;
  recordedBlobs = [];
  recordedVideoUrl.value = null;
  isRecordingTest.value = true;

  try {
    const options = { mimeType: "video/webm;codecs=vp9,opus", videoBitsPerSecond: 16000000 };
    mediaRecorder = new MediaRecorder(activeStream, options);

    mediaRecorder.ondataavailable = (e) => {
      if (e.data && e.data.size > 0) recordedBlobs.push(e.data);
    };

    mediaRecorder.onstop = () => {
      const superBuffer = new Blob(recordedBlobs, { type: "video/webm" });
      recordedVideoUrl.value = URL.createObjectURL(superBuffer);
      isRecordingTest.value = false;
      showToast("Teste de 5s concluído!");
    };

    mediaRecorder.start(100);
    showToast("Gravando teste de 5 segundos...");

    setTimeout(() => {
      if (mediaRecorder && mediaRecorder.state === "recording") {
        mediaRecorder.stop();
      }
    }, 5000);
  } catch (err) {
    console.error(err);
    isRecordingTest.value = false;
  }
}

// ----------------- PROCESS TELEMETRY -----------------
async function fetchStats() {
  try {
    processStats.value = await invoke<ProcessStats>("get_process_stats");
  } catch (_) {}
}

onMounted(async () => {
  await refreshDevices();
  fetchStats();
  statsTimer = setInterval(fetchStats, 3000);
  startLivePreview();

  document.addEventListener("fullscreenchange", () => {
    isFullscreen.value = !!document.fullscreenElement;
    if (!isFullscreen.value) {
      showControlsOverlay.value = true;
      if (hideControlsTimer) clearTimeout(hideControlsTimer);
    } else {
      resetHideTimer();
    }
  });

  window.addEventListener("mousemove", resetHideTimer);
  window.addEventListener("pointermove", resetHideTimer);
  window.addEventListener("mousedown", resetHideTimer);

  window.addEventListener("keydown", (e) => {
    resetHideTimer();
    if (e.key === "f" || e.key === "F11") {
      e.preventDefault();
      toggleFullscreen();
    }
    if (e.key === "m" || e.key === "M") {
      toggleAudioMute();
    }
    if (e.key === "s" || e.key === "S") {
      isSidebarOpen.value = !isSidebarOpen.value;
      if (isSidebarOpen.value) showControlsOverlay.value = true;
    }
  });
});

onUnmounted(() => {
  stopLivePreview();
  if (statsTimer) clearInterval(statsTimer);
  if (hideControlsTimer) clearTimeout(hideControlsTimer);
  window.removeEventListener("mousemove", resetHideTimer);
  window.removeEventListener("pointermove", resetHideTimer);
  window.removeEventListener("mousedown", resetHideTimer);
});
</script>

<template>
  <div
    class="studio-app"
    :class="{ 'cursor-hidden': isFullscreen && !showControlsOverlay }"
    @mousemove="resetHideTimer"
    @click="resetHideTimer"
  >
    <!-- Top Sleek Bar (Clean Obsidian Header) -->
    <header class="top-nav" :class="{ 'autohide-nav': isFullscreen && !showControlsOverlay }">
      <div class="nav-brand">
        <!-- UrsoCapture Bear Logo SVG -->
        <svg class="urso-brand-logo" viewBox="0 0 512 512" fill="none">
          <defs>
            <linearGradient id="navBearGrad" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stop-color="#00f2fe" />
              <stop offset="100%" stop-color="#7928ca" />
            </linearGradient>
          </defs>
          <circle cx="140" cy="150" r="56" fill="#141d30" stroke="url(#navBearGrad)" stroke-width="16" />
          <circle cx="372" cy="150" r="56" fill="#141d30" stroke="url(#navBearGrad)" stroke-width="16" />
          <path d="M150 180 L362 180 L410 280 L350 410 L256 450 L162 410 L102 280 Z" fill="#0d1424" stroke="url(#navBearGrad)" stroke-width="18" stroke-linejoin="round" />
          <polygon points="175,260 225,260 215,280 185,280" fill="#00f2fe" />
          <polygon points="287,260 337,260 327,280 297,280" fill="#00f2fe" />
          <circle cx="256" cy="330" r="38" fill="#090d18" stroke="url(#navBearGrad)" stroke-width="10" />
          <circle cx="256" cy="330" r="14" fill="#ff0055" />
        </svg>
        <span class="brand-title">UrsoCapture</span>
        <span class="brand-badge">STUDIO AV</span>
      </div>

      <div class="nav-telemetry">
        <!-- Live Resolution & FPS -->
        <div class="status-pill" v-if="isStreamActive">
          <span class="pill-dot"></span>
          <span class="pill-txt font-mono">{{ liveVideoWidth }}x{{ liveVideoHeight }} @ {{ liveFps }} FPS</span>
        </div>

        <!-- 0 Delay Badge -->
        <div class="status-pill delay-pill">
          <span class="pill-txt">0 DELAY PASSTHROUGH</span>
        </div>

        <!-- Real RAM -->
        <div class="status-pill ram-pill">
          <span class="pill-txt font-mono">RAM: {{ processStats ? `${processStats.ram_working_set_mb.toFixed(1)}MB` : '26MB' }}</span>
        </div>

        <!-- Settings Drawer Toggle Button -->
        <button
          class="nav-btn config-btn"
          :class="{ 'btn-active': isSidebarOpen }"
          @click="isSidebarOpen = !isSidebarOpen"
          title="Abrir Painel de Configurações (Atalho: S)"
        >
          <span class="btn-icon">⚙️</span>
          <span>Configurações</span>
        </button>
      </div>
    </header>

    <!-- Main Live Screen Area (Zero Margins, Pure Black) -->
    <div
      ref="videoContainerRef"
      class="video-stage"
      @dblclick="toggleFullscreen"
    >
      <!-- Video Element with Pixel-Perfect Scaling & Anti-Border Zoom -->
      <video
        ref="videoElementRef"
        autoplay
        playsinline
        muted
        class="live-video-element"
        :class="[
          `fit-${fitMode}`,
          { 'is-mirrored': mirrorVideo }
        ]"
        :style="{
          transform: `scale(${videoZoom / 100}) ${mirrorVideo ? 'scaleX(-1)' : ''}`,
          filter: `brightness(${brightness}%) contrast(${contrast}%) saturate(${saturation}%)`
        }"
      ></video>

      <!-- Rule of Thirds Alignment Grid (Optional) -->
      <div v-if="showGrid" class="framing-grid">
        <div class="grid-line gh1"></div>
        <div class="grid-line gh2"></div>
        <div class="grid-line gv1"></div>
        <div class="grid-line gv2"></div>
      </div>

      <!-- Inactive Stream Alert -->
      <div v-if="!isStreamActive" class="standby-screen">
        <div class="standby-box">
          <div class="standby-icon">🎮</div>
          <h2>Aguardando Sinal de Captura</h2>
          <p>{{ streamError || 'Conecte sua placa de captura ou console e inicie o preview.' }}</p>
          <button class="primary-btn" @click="startLivePreview">
            ▶ Iniciar Transmissão
          </button>
        </div>
      </div>

      <!-- Floating Bottom Quick-Control HUD -->
      <div
        class="floating-hud"
        :class="{ 'hud-hidden': isFullscreen && !showControlsOverlay }"
        @click.stop
      >
        <!-- Audio Monitor & VU Meter -->
        <div class="hud-group audio-group">
          <button
            class="hud-icon-btn"
            :class="{ 'btn-muted': !isAudioMonitoringActive }"
            @click="toggleAudioMute"
            :title="isAudioMonitoringActive ? 'Silenciar Retorno (M)' : 'Ativar Retorno (M)'"
          >
            {{ isAudioMonitoringActive ? '🔊' : '🔇' }}
          </button>

          <!-- Volume Slider -->
          <input
            type="range"
            min="0"
            max="100"
            v-model="monitorVolume"
            @input="updateMonitoringVolume"
            class="hud-volume-slider"
            :title="`Volume: ${monitorVolume}%`"
          />

          <!-- Live Mini VU Meter -->
          <div class="hud-vu-box" title="Nível de Áudio dBFS">
            <div
              class="hud-vu-bar"
              :class="{ 'vu-clip': isAudioClipping }"
              :style="{ width: `${Math.max(0, ((audioDbLevel + 60) / 60) * 100)}%` }"
            ></div>
          </div>
          <span class="hud-db-text font-mono">{{ audioDbLevel }}dB</span>
        </div>

        <div class="hud-divider"></div>

        <!-- Anti-Border & Fit Modes -->
        <div class="hud-group fit-group">
          <span class="hud-label">Enquadramento:</span>
          <button
            class="hud-chip"
            :class="{ active: fitMode === 'cover' }"
            @click="fitMode = 'cover'"
            title="Preencher 100% da tela (Remove bordas brancas e pretas)"
          >
            Preencher
          </button>
          <button
            class="hud-chip"
            :class="{ active: fitMode === 'contain' }"
            @click="fitMode = 'contain'"
            title="Ajustar 16:9 Original"
          >
            Ajustar
          </button>
          <button
            class="hud-chip"
            :class="{ active: fitMode === 'fill' }"
            @click="fitMode = 'fill'"
            title="Esticar Janela"
          >
            Esticar
          </button>
        </div>

        <div class="hud-divider"></div>

        <!-- Zoom / Overscan Crop Slider (To eliminate dongle borders) -->
        <div class="hud-group zoom-group">
          <span class="hud-label">Zoom/Corte: {{ videoZoom }}%</span>
          <input
            type="range"
            min="100"
            max="115"
            step="1"
            v-model="videoZoom"
            class="hud-zoom-slider"
            title="Ajuste fino para cortar bordas do dongle de captura"
          />
        </div>

        <div class="hud-divider"></div>

        <!-- Quick Utility Tools -->
        <div class="hud-group tools-group">
          <button
            class="hud-btn"
            :class="{ active: mirrorVideo }"
            @click="mirrorVideo = !mirrorVideo"
            title="Espelhar Imagem"
          >
            🪞 Espelhar
          </button>
          <button class="hud-btn" @click="takeSnapshot" title="Capturar Frame em Alta Definição">
            📸 Foto HD
          </button>
          <button
            class="hud-btn"
            :class="{ 'btn-recording': isRecordingTest }"
            @click="startSyncTestRecording"
            title="Gravar 5 segundos para testar sincronia perfeita"
          >
            {{ isRecordingTest ? '● Gravando...' : '🎬 Testar Sincronia' }}
          </button>
          <button class="hud-btn" @click="toggleFullscreen" title="Tela Cheia (F11 ou Duplo Clique)">
            ⛶ Tela Cheia
          </button>
        </div>
      </div>
    </div>

    <!-- Right Drawer: Comprehensive Configuration Sidebar -->
    <aside class="settings-drawer" :class="{ 'drawer-open': isSidebarOpen }">
      <div class="drawer-header">
        <div class="drawer-title">
          <span class="drawer-icon">⚙️</span>
          <h2>Configuração de Entradas</h2>
        </div>
        <button class="close-drawer-btn" @click="isSidebarOpen = false">✕</button>
      </div>

      <div class="drawer-body">
        <!-- Video Source -->
        <div class="setting-block">
          <label class="block-label">📹 Dispositivo de Entrada de Vídeo</label>
          <select v-model="selectedVideoId" class="drawer-select" @change="startLivePreview">
            <option v-for="dev in videoDevices" :key="dev.deviceId" :value="dev.deviceId">
              {{ dev.label }}
            </option>
            <option v-if="videoDevices.length === 0" value="">Nenhuma câmera detectada</option>
          </select>
          <span class="block-hint">Placas de captura USB HDMI, Webcams e OBS Virtual Camera.</span>
        </div>

        <!-- Audio Source -->
        <div class="setting-block">
          <label class="block-label">🎙️ Dispositivo de Entrada de Áudio</label>
          <select v-model="selectedAudioId" class="drawer-select" @change="startLivePreview">
            <option v-for="dev in audioDevices" :key="dev.deviceId" :value="dev.deviceId">
              {{ dev.label }}
            </option>
            <option v-if="audioDevices.length === 0" value="">Nenhum microfone detectado</option>
          </select>
          <span class="block-hint">Áudio da placa de captura, microfones e interfaces USB.</span>
        </div>

        <!-- Target Resolution -->
        <div class="setting-block">
          <label class="block-label">📺 Resolução de Captura</label>
          <div class="pill-row">
            <button
              v-for="r in [
                { id: '1080p', name: '1080p FHD' },
                { id: '4k', name: '4K UHD' },
                { id: '720p', name: '720p HD' },
                { id: 'max', name: 'Máxima' }
              ]"
              :key="r.id"
              class="pill-button"
              :class="{ active: targetResolution === r.id }"
              @click="targetResolution = r.id; startLivePreview();"
            >
              {{ r.name }}
            </button>
          </div>
        </div>

        <!-- Target Framerate -->
        <div class="setting-block">
          <label class="block-label">⚡ Taxa de Quadros (Target FPS)</label>
          <div class="pill-row">
            <button
              v-for="fps in [60, 30, 24]"
              :key="fps"
              class="pill-button"
              :class="{ active: targetFps === fps }"
              @click="targetFps = fps; startLivePreview();"
            >
              {{ fps }} FPS
            </button>
          </div>
        </div>

        <!-- Studio Raw Toggle -->
        <div class="setting-block raw-card">
          <div class="raw-header">
            <label class="switch-toggle">
              <input type="checkbox" v-model="rawStudioAudio" @change="startLivePreview" />
              <span class="slider"></span>
            </label>
            <div>
              <strong>Modo Studio Raw (Zero Delay)</strong>
              <p>Desativa cancelamento de eco e redutores de ruído para latência nula e som cristalino.</p>
            </div>
          </div>
        </div>

        <!-- Image Calibration (Color & Contrast) -->
        <div class="setting-block">
          <label class="block-label">🎨 Calibração de Imagem do Monitor</label>
          <div class="slider-field">
            <span>Brilho: {{ brightness }}%</span>
            <input type="range" min="50" max="150" v-model="brightness" />
          </div>
          <div class="slider-field">
            <span>Contraste: {{ contrast }}%</span>
            <input type="range" min="50" max="150" v-model="contrast" />
          </div>
          <div class="slider-field">
            <span>Saturação: {{ saturation }}%</span>
            <input type="range" min="50" max="150" v-model="saturation" />
          </div>
          <button class="reset-calib-btn" @click="brightness = 100; contrast = 100; saturation = 100; videoZoom = 100;">
            ↺ Resetar Calibração
          </button>
        </div>

        <!-- Audio Frequency Visualizer -->
        <div class="setting-block">
          <label class="block-label">📊 Espectrograma de Áudio em Tempo Real</label>
          <div class="canvas-box">
            <canvas ref="audioVisualizerCanvasRef" width="300" height="40" class="drawer-canvas"></canvas>
          </div>
        </div>

        <!-- Reconnect Action Button -->
        <button class="primary-btn full-btn" @click="refreshDevices(); startLivePreview();">
          🔄 Atualizar e Reconectar
        </button>
      </div>
    </aside>

    <!-- Recorded Test Playback Modal -->
    <div v-if="recordedVideoUrl" class="modal-overlay" @click="recordedVideoUrl = null">
      <div class="modal-card" @click.stop>
        <div class="modal-head">
          <h3>🎬 Teste de Sincronia A/V (5s)</h3>
          <button class="close-btn" @click="recordedVideoUrl = null">✕</button>
        </div>
        <video :src="recordedVideoUrl" controls autoplay class="modal-video"></video>
        <p class="modal-hint">Reproduza o vídeo para verificar a sincronização exata entre imagem e som.</p>
      </div>
    </div>

    <!-- Floating Toast Notification -->
    <transition name="fade">
      <div v-if="toastMessage" class="toast-card">
        <span>✨ {{ toastMessage }}</span>
      </div>
    </transition>
  </div>
</template>

<style scoped>
/* Reset & Shell */
.studio-app {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  width: 100vw;
  height: 100vh;
  margin: 0;
  padding: 0;
  background: #000000;
  color: #f1f5f9;
  font-family: 'Plus Jakarta Sans', system-ui, sans-serif;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  user-select: none;
  cursor: default;
}

/* Hide Mouse Cursor in Fullscreen when Inactive */
.studio-app.cursor-hidden,
.studio-app.cursor-hidden * {
  cursor: none !important;
}

/* Top Sleek Obsidian Navigation */
.top-nav {
  height: 44px;
  min-height: 44px;
  max-height: 44px;
  background: rgba(8, 12, 20, 0.96);
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 16px;
  z-index: 50;
  flex-shrink: 0;
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.25s ease, margin-top 0.25s ease;
}

.autohide-nav {
  margin-top: -44px;
  opacity: 0;
  pointer-events: none;
}

.nav-brand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.urso-brand-logo {
  width: 26px;
  height: 26px;
  filter: drop-shadow(0 0 8px rgba(0, 242, 254, 0.5));
}

.brand-title {
  font-size: 0.98rem;
  font-weight: 800;
  letter-spacing: -0.02em;
  background: linear-gradient(135deg, #ffffff, #cbd5e1);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}

.brand-badge {
  font-size: 0.68rem;
  font-weight: 800;
  background: #00f2fe;
  color: #030e24;
  padding: 2px 6px;
  border-radius: 4px;
}

.nav-telemetry {
  display: flex;
  align-items: center;
  gap: 10px;
}

.status-pill {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 20px;
  font-size: 0.74rem;
}

.pill-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #00f2fe;
}

.delay-pill {
  border-color: rgba(16, 185, 129, 0.3);
  color: #10b981;
  font-weight: 700;
}

.ram-pill {
  color: #94a3b8;
}

.nav-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: #f1f5f9;
  border-radius: 8px;
  font-size: 0.78rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.nav-btn:hover, .nav-btn.btn-active {
  background: #00f2fe;
  color: #030e24;
  border-color: #00f2fe;
}

/* Main Video Stage (Pure Black Zero Borders) */
.video-stage {
  flex: 1;
  width: 100%;
  height: 100%;
  min-height: 0;
  position: relative;
  background: #000000;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  margin: 0;
  padding: 0;
}

/* Video Element & Fitting Modes */
.live-video-element {
  width: 100%;
  height: 100%;
  display: block;
  background: #000000;
  border: none;
  outline: none;
  margin: 0;
  padding: 0;
  transform-origin: center center;
  transition: transform 0.15s ease-out;
}

.live-video-element.fit-cover {
  object-fit: cover;
}

.live-video-element.fit-contain {
  object-fit: contain;
}

.live-video-element.fit-fill {
  object-fit: fill;
}

/* Framing Grid */
.framing-grid {
  position: absolute;
  top: 0; left: 0; right: 0; bottom: 0;
  pointer-events: none;
}

.grid-line {
  position: absolute;
  background: rgba(255, 255, 255, 0.15);
}

.grid-line.gh1 { top: 33.33%; left: 0; right: 0; height: 1px; }
.grid-line.gh2 { top: 66.66%; left: 0; right: 0; height: 1px; }
.grid-line.gv1 { left: 33.33%; top: 0; bottom: 0; width: 1px; }
.grid-line.gv2 { left: 66.66%; top: 0; bottom: 0; width: 1px; }

/* Standby Screen */
.standby-screen {
  position: absolute;
  top: 0; left: 0; right: 0; bottom: 0;
  background: #060911;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
}

.standby-box {
  max-width: 400px;
  padding: 24px;
}

.standby-icon {
  font-size: 3rem;
  margin-bottom: 12px;
}

.standby-box h2 {
  font-size: 1.3rem;
  margin: 0 0 8px 0;
}

.standby-box p {
  color: #94a3b8;
  font-size: 0.86rem;
  margin: 0 0 18px 0;
}

/* Floating Bottom HUD */
.floating-hud {
  position: absolute;
  bottom: 20px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(10, 15, 26, 0.85);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 30px;
  padding: 6px 16px;
  display: flex;
  align-items: center;
  gap: 12px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.6);
  z-index: 40;
  transition: opacity 0.3s ease, transform 0.3s ease;
}

.hud-hidden {
  opacity: 0;
  pointer-events: none;
  transform: translate(-50%, 20px);
}

.hud-group {
  display: flex;
  align-items: center;
  gap: 8px;
}

.hud-label {
  font-size: 0.72rem;
  font-weight: 700;
  color: #94a3b8;
}

.hud-divider {
  width: 1px;
  height: 20px;
  background: rgba(255, 255, 255, 0.1);
}

.hud-icon-btn {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: #f1f5f9;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.85rem;
  cursor: pointer;
}

.hud-icon-btn.btn-muted {
  background: rgba(239, 68, 68, 0.2);
  border-color: #ef4444;
}

.hud-volume-slider {
  width: 70px;
  accent-color: #00f2fe;
}

.hud-zoom-slider {
  width: 80px;
  accent-color: #00f2fe;
}

.hud-vu-box {
  width: 48px;
  height: 8px;
  background: rgba(0, 0, 0, 0.6);
  border-radius: 4px;
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.hud-vu-bar {
  height: 100%;
  background: linear-gradient(90deg, #10b981 0%, #00f2fe 80%, #ef4444 100%);
  transition: width 0.06s ease-out;
}

.hud-vu-bar.vu-clip {
  background: #ef4444;
}

.hud-db-text {
  font-size: 0.68rem;
  color: #94a3b8;
  width: 32px;
}

.hud-chip {
  padding: 4px 10px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 12px;
  font-size: 0.72rem;
  font-weight: 600;
  color: #cbd5e1;
  cursor: pointer;
  transition: all 0.15s;
}

.hud-chip:hover {
  background: rgba(255, 255, 255, 0.12);
}

.hud-chip.active {
  background: #00f2fe;
  color: #030e24;
  font-weight: 700;
  border-color: #00f2fe;
}

.hud-btn {
  padding: 5px 10px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  font-size: 0.72rem;
  font-weight: 600;
  color: #f1f5f9;
  cursor: pointer;
  transition: all 0.15s;
}

.hud-btn:hover, .hud-btn.active {
  background: rgba(0, 242, 254, 0.2);
  border-color: #00f2fe;
  color: #00f2fe;
}

.hud-btn.btn-recording {
  background: rgba(239, 68, 68, 0.3);
  border-color: #ef4444;
  color: #ef4444;
}

/* Settings Drawer */
.settings-drawer {
  position: fixed;
  top: 0;
  right: 0;
  width: 340px;
  height: 100vh;
  background: rgba(10, 15, 26, 0.96);
  backdrop-filter: blur(20px);
  border-left: 1px solid rgba(255, 255, 255, 0.1);
  display: flex;
  flex-direction: column;
  z-index: 100;
  transform: translateX(100%);
  transition: transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  box-shadow: -10px 0 40px rgba(0, 0, 0, 0.6);
}

.settings-drawer.drawer-open {
  transform: translateX(0);
}

.drawer-header {
  padding: 16px 20px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
}

.drawer-title {
  display: flex;
  align-items: center;
  gap: 8px;
}

.drawer-title h2 {
  font-size: 1rem;
  font-weight: 700;
  margin: 0;
}

.close-drawer-btn {
  background: transparent;
  border: none;
  color: #94a3b8;
  font-size: 1.1rem;
  cursor: pointer;
}

.drawer-body {
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
}

.setting-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.block-label {
  font-size: 0.78rem;
  font-weight: 700;
  color: #cbd5e1;
}

.drawer-select {
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: #f1f5f9;
  padding: 8px 10px;
  border-radius: 8px;
  font-size: 0.82rem;
  outline: none;
}

.drawer-select:focus {
  border-color: #00f2fe;
}

.block-hint {
  font-size: 0.7rem;
  color: #64748b;
}

.pill-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(70px, 1fr));
  gap: 6px;
}

.pill-button {
  padding: 6px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 6px;
  font-size: 0.74rem;
  font-weight: 600;
  color: #cbd5e1;
  cursor: pointer;
}

.pill-button.active {
  background: #00f2fe;
  color: #030e24;
  font-weight: 700;
  border-color: #00f2fe;
}

.raw-card {
  background: rgba(0, 242, 254, 0.06);
  border: 1px solid rgba(0, 242, 254, 0.2);
  padding: 10px;
  border-radius: 8px;
}

.raw-header {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}

.raw-header strong {
  font-size: 0.8rem;
  color: #00f2fe;
  display: block;
}

.raw-header p {
  font-size: 0.72rem;
  color: #94a3b8;
  margin: 2px 0 0 0;
}

.slider-field {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.74rem;
  color: #94a3b8;
}

.slider-field input {
  width: 130px;
  accent-color: #00f2fe;
}

.reset-calib-btn {
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: #94a3b8;
  padding: 4px 8px;
  border-radius: 6px;
  font-size: 0.72rem;
  cursor: pointer;
  margin-top: 4px;
}

.canvas-box {
  background: rgba(0, 0, 0, 0.4);
  border-radius: 6px;
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.drawer-canvas {
  width: 100%;
  height: 40px;
  display: block;
}

.primary-btn {
  background: linear-gradient(135deg, #00f2fe, #4facfe);
  color: #030e24;
  border: none;
  padding: 10px 16px;
  border-radius: 8px;
  font-size: 0.84rem;
  font-weight: 700;
  cursor: pointer;
  transition: all 0.2s;
}

.primary-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 15px rgba(0, 242, 254, 0.3);
}

.full-btn {
  width: 100%;
  margin-top: 8px;
}

/* Switch Toggle Component */
.switch-toggle {
  position: relative;
  display: inline-block;
  width: 32px;
  height: 18px;
  flex-shrink: 0;
}

.switch-toggle input {
  opacity: 0;
  width: 0;
  height: 0;
}

.slider {
  position: absolute;
  top: 0; left: 0; right: 0; bottom: 0;
  background: #334155;
  border-radius: 20px;
  cursor: pointer;
  transition: 0.3s;
}

.slider:before {
  position: absolute;
  content: "";
  height: 12px;
  width: 12px;
  left: 3px;
  bottom: 3px;
  background: white;
  border-radius: 50%;
  transition: 0.3s;
}

input:checked + .slider {
  background: #00f2fe;
}

input:checked + .slider:before {
  transform: translateX(14px);
}

/* Modal Overlay */
.modal-overlay {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  background: rgba(0, 0, 0, 0.85);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
}

.modal-card {
  background: #0f172a;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 12px;
  padding: 16px;
  max-width: 640px;
  width: 90%;
}

.modal-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
}

.modal-head h3 {
  margin: 0;
  font-size: 1rem;
}

.close-btn {
  background: transparent;
  border: none;
  color: #94a3b8;
  font-size: 1.1rem;
  cursor: pointer;
}

.modal-video {
  width: 100%;
  max-height: 360px;
  border-radius: 8px;
  background: #000;
}

.modal-hint {
  font-size: 0.78rem;
  color: #94a3b8;
  margin: 8px 0 0 0;
}

/* Toast */
.toast-card {
  position: fixed;
  bottom: 24px;
  right: 24px;
  background: #0f172a;
  border: 1px solid #00f2fe;
  padding: 10px 18px;
  border-radius: 8px;
  font-size: 0.82rem;
  font-weight: 600;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.6);
  z-index: 9999;
}

.font-mono { font-family: 'JetBrains Mono', monospace; }
.fade-enter-active, .fade-leave-active { transition: opacity 0.25s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>