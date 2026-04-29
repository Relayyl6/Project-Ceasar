(function () {
  "use strict";

  const infraCanvas = document.getElementById("infraCanvas");
  if (!infraCanvas) {
    return;
  }

  const infraCtx = infraCanvas.getContext("2d");
  const vibCanvas = document.getElementById("vibCanvas");
  const vibCtx = vibCanvas.getContext("2d");
  const presCanvas = document.getElementById("presCanvas");
  const presCtx = presCanvas.getContext("2d");

  const state = {
    stats: {},
    alerts: [],
    nodeRegistry: { nodes: [] },
    orchestration: {},
    online: false,
    lastSyncMs: null,
    sim: {},
  };

  const vibHistory = Array(90).fill(0.018);
  const pressureHistory = Array(90).fill(14.2);
  let epochTimer = null;

  function formatTime(ms) {
    if (!ms) return "--:--";
    return new Date(ms).toLocaleTimeString([], {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function hashString(input) {
    let hash = 0;
    for (const char of String(input || "")) {
      hash = (hash << 5) - hash + char.charCodeAt(0);
      hash |= 0;
    }
    return Math.abs(hash);
  }

  function fallbackCoord(nodeId, index) {
    const seed = hashString(`${nodeId}-${index}`);
    return {
      x: 0.14 + (seed % 70) / 100,
      y: 0.2 + ((Math.floor(seed / 10) % 55) / 100),
    };
  }

  function currentAlert() {
    return state.alerts[0]?.envelope?.body || null;
  }

  function pipelineNodes() {
    const nodes = state.nodeRegistry?.nodes || [];
    if (!nodes.length) {
      return [
        { node_id: "tower-bwari-alpha", role: "fixed_tower" },
        { node_id: "tower-bwari-bravo", role: "fixed_tower" },
        { node_id: "drone-relay-01", role: "relay" },
        { node_id: "hub-bwari-01", role: "regional_hub" },
      ].map((node, index) => ({
        ...node,
        ...fallbackCoord(node.node_id, index),
      }));
    }

    return nodes.map((node, index) => {
      const coord = fallbackCoord(node.node_id, index);
      if (node.role === "regional_hub") {
        return { ...node, x: 0.5, y: 0.52 };
      }
      if (node.role === "relay") {
        return { ...node, x: 0.62, y: 0.32 };
      }
      return { ...node, ...coord };
    });
  }

  function resizeCanvases() {
    infraCanvas.width = infraCanvas.offsetWidth;
    infraCanvas.height = infraCanvas.offsetHeight;
    vibCanvas.width = vibCanvas.offsetWidth;
    vibCanvas.height = vibCanvas.offsetHeight || 100;
    presCanvas.width = presCanvas.offsetWidth;
    presCanvas.height = presCanvas.offsetHeight || 90;
  }

  function drawInfraMap() {
    if (
      infraCanvas.width !== infraCanvas.offsetWidth ||
      infraCanvas.height !== infraCanvas.offsetHeight
    ) {
      resizeCanvases();
    }

    const width = infraCanvas.width;
    const height = infraCanvas.height;
    const nodes = pipelineNodes();
    const hub = nodes.find((node) => node.role === "regional_hub") || nodes[0];
    const alert = currentAlert();
    const anomalyProbability = Number(
      state.stats.anomaly_probability ?? state.sim.anomalyProb ?? 0.04
    );

    infraCtx.fillStyle = "#08101a";
    infraCtx.fillRect(0, 0, width, height);

    infraCtx.strokeStyle = "rgba(0,204,255,0.05)";
    infraCtx.lineWidth = 0.5;
    for (let x = 0; x < width; x += 40) {
      infraCtx.beginPath();
      infraCtx.moveTo(x, 0);
      infraCtx.lineTo(x, height);
      infraCtx.stroke();
    }
    for (let y = 0; y < height; y += 40) {
      infraCtx.beginPath();
      infraCtx.moveTo(0, y);
      infraCtx.lineTo(width, y);
      infraCtx.stroke();
    }

    nodes.forEach((node) => {
      if (!hub || node.node_id === hub.node_id) {
        return;
      }
      const isAlertNode = alert && alert.node_id === node.node_id;
      infraCtx.strokeStyle = isAlertNode
        ? "rgba(255,187,0,0.5)"
        : node.active
        ? "rgba(0,204,255,0.26)"
        : "rgba(0,204,255,0.12)";
      infraCtx.lineWidth = isAlertNode ? 2.2 : 1.4;
      infraCtx.beginPath();
      infraCtx.moveTo(hub.x * width, hub.y * height);
      infraCtx.quadraticCurveTo(
        ((hub.x + node.x) / 2) * width,
        ((hub.y + node.y) / 2 - 0.1) * height,
        node.x * width,
        node.y * height
      );
      infraCtx.stroke();
    });

    nodes.forEach((node) => {
      const isAlertNode = alert && alert.node_id === node.node_id;
      const glow = infraCtx.createRadialGradient(
        node.x * width,
        node.y * height,
        0,
        node.x * width,
        node.y * height,
        isAlertNode ? 28 : 18
      );
      glow.addColorStop(
        0,
        isAlertNode
          ? `rgba(255,187,0,${0.18 + anomalyProbability * 2})`
          : "rgba(0,204,255,0.18)"
      );
      glow.addColorStop(1, "rgba(0,204,255,0)");
      infraCtx.fillStyle = glow;
      infraCtx.beginPath();
      infraCtx.arc(node.x * width, node.y * height, isAlertNode ? 28 : 18, 0, Math.PI * 2);
      infraCtx.fill();

      infraCtx.fillStyle = isAlertNode ? "#ffbb00" : node.active ? "#00ccff" : "#3a5565";
      infraCtx.beginPath();
      infraCtx.arc(node.x * width, node.y * height, isAlertNode ? 5 : 4, 0, Math.PI * 2);
      infraCtx.fill();

      infraCtx.fillStyle = "rgba(7,10,15,0.92)";
      infraCtx.fillRect(node.x * width + 8, node.y * height - 12, 114, 16);
      infraCtx.strokeStyle = isAlertNode ? "#ffbb00" : "rgba(0,204,255,0.4)";
      infraCtx.lineWidth = 0.8;
      infraCtx.strokeRect(node.x * width + 8, node.y * height - 12, 114, 16);
      infraCtx.fillStyle = isAlertNode ? "#ffbb00" : "#00ccff";
      infraCtx.font = "8px Share Tech Mono";
      infraCtx.fillText(String(node.node_id || "").toUpperCase(), node.x * width + 14, node.y * height - 1);
    });

    window.requestAnimationFrame(drawInfraMap);
  }

  function drawBars() {
    const vibValue = Number(state.stats.vibration_rms ?? state.sim.vibration ?? 0.018);
    const pressureValue = Number(
      state.stats.pipeline_pressure_bar ?? state.sim.pressure ?? 14.2
    );

    vibHistory.push(vibValue);
    vibHistory.shift();
    pressureHistory.push(pressureValue);
    pressureHistory.shift();

    vibCtx.fillStyle = "#08101a";
    vibCtx.fillRect(0, 0, vibCanvas.width, vibCanvas.height);
    const vibBarWidth = vibCanvas.width / vibHistory.length;
    vibHistory.forEach((value, index) => {
      const scaledHeight = (value / 0.06) * vibCanvas.height * 0.85;
      vibCtx.fillStyle =
        index === vibHistory.length - 1
          ? "#00ccff"
          : `rgba(0,204,255,${0.18 + (index / vibHistory.length) * 0.38})`;
      vibCtx.fillRect(
        index * vibBarWidth,
        vibCanvas.height - scaledHeight,
        Math.max(1, vibBarWidth - 1),
        scaledHeight
      );
    });

    const refY = vibCanvas.height - (0.02 / 0.06) * vibCanvas.height * 0.85;
    vibCtx.strokeStyle = "rgba(0,204,255,0.2)";
    vibCtx.setLineDash([4, 4]);
    vibCtx.beginPath();
    vibCtx.moveTo(0, refY);
    vibCtx.lineTo(vibCanvas.width, refY);
    vibCtx.stroke();
    vibCtx.setLineDash([]);

    presCtx.fillStyle = "#08101a";
    presCtx.fillRect(0, 0, presCanvas.width, presCanvas.height);
    const gradient = presCtx.createLinearGradient(0, 0, 0, presCanvas.height);
    gradient.addColorStop(0, "rgba(0,238,136,0.22)");
    gradient.addColorStop(1, "rgba(0,238,136,0)");
    presCtx.fillStyle = gradient;
    presCtx.beginPath();
    presCtx.moveTo(0, presCanvas.height);
    pressureHistory.forEach((value, index) => {
      const x = (index / (pressureHistory.length - 1)) * presCanvas.width;
      const y =
        presCanvas.height -
        ((value - 8) / (20 - 8)) * presCanvas.height * 0.82 -
        presCanvas.height * 0.06;
      presCtx.lineTo(x, y);
    });
    presCtx.lineTo(presCanvas.width, presCanvas.height);
    presCtx.fill();

    presCtx.strokeStyle = "#00ee88";
    presCtx.lineWidth = 1.5;
    presCtx.beginPath();
    pressureHistory.forEach((value, index) => {
      const x = (index / (pressureHistory.length - 1)) * presCanvas.width;
      const y =
        presCanvas.height -
        ((value - 8) / (20 - 8)) * presCanvas.height * 0.82 -
        presCanvas.height * 0.06;
      if (index === 0) {
        presCtx.moveTo(x, y);
      } else {
        presCtx.lineTo(x, y);
      }
    });
    presCtx.stroke();
  }

  function setText(id, value) {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = value;
    }
  }

  function renderEvents() {
    const eventLog = document.getElementById("eventLog");
    if (!eventLog) {
      return;
    }

    const items = state.alerts.length
      ? state.alerts.slice(0, 3).map((record) => {
          const body = record.envelope?.body || {};
          return {
            time: formatTime(record.received_at_ms || body.timestamp_ms),
            title: `${String(body.track_id || "ALERT").toUpperCase()} ${String(
              body.threat_level || "monitor"
            ).toUpperCase()}`,
            desc: `${String(body.node_id || "unknown").toUpperCase()} reported ${String(
              body.site || "unknown-site"
            ).replaceAll("-", " ")} at confidence ${Number(body.confidence || 0).toFixed(3)}.`,
          };
        })
      : [
          {
            time: formatTime(state.lastSyncMs),
            title: "NO ACTIVE INCIDENTS",
            desc: "Infrastructure telemetry is nominal. Relay posture and pressure bands remain inside the rolling threshold window.",
          },
        ];

    eventLog.innerHTML = "";
    items.forEach((item) => {
      const node = document.createElement("div");
      node.className = "event-item";
      node.innerHTML = `
        <div style="text-align:right;flex-shrink:0">
          <div class="event-time">${item.time}</div>
          <div class="event-time-sub">UTC</div>
        </div>
        <div>
          <div class="event-title">${item.title}</div>
          <div class="event-desc">${item.desc}</div>
        </div>
      `;
      eventLog.appendChild(node);
    });
  }

  function renderSwarm() {
    const swarmGrid = document.getElementById("swarmGrid");
    if (!swarmGrid) {
      return;
    }

    const units = (state.nodeRegistry?.nodes || []).slice(0, 4);
    swarmGrid.innerHTML = "";
    units.forEach((unit) => {
      const cell = document.createElement("div");
      cell.className = "swarm-unit";
      cell.innerHTML = `
        <div class="swarm-icon-ring">
          <span class="material-symbols-outlined" style="font-size:16px;color:var(--cyan)">precision_manufacturing</span>
          ${unit.active ? '<div class="swarm-status-dot"></div>' : ""}
        </div>
        <div class="swarm-unit-label">${String(unit.node_id || "").toUpperCase()}</div>
        <div class="swarm-unit-sub">${String(unit.zone || "mesh-zone").replaceAll("-", " ").toUpperCase()} // ${
        unit.active ? "ACTIVE" : "STANDBY"
      }</div>
      `;
      swarmGrid.appendChild(cell);
    });
  }

  function renderKernelLog() {
    const target = document.getElementById("ebpfLog");
    if (!target) {
      return;
    }

    const alert = currentAlert();
    const digest = state.orchestration?.policy_digest || {};
    const lines = [
      `authorized kprobe attached: pressure_controller_write`,
      `signed sensor stream synchronized from ${alert?.node_id || "hub-bwari-01"}`,
      `regional exchange protocol: ${digest.regional_exchange_protocol || "amqp"}`,
      `routing priority: ${digest.high_priority_protocol || "dds"}`,
      `alarm score: ${Number(state.stats.vibration_rms ?? state.sim.vibration ?? 0.018).toFixed(3)} rms`,
      `heartbeat: kernel telemetry audit healthy`,
    ];

    target.innerHTML = lines
      .map(
        (line, index) =>
          `<div class="ebpf-line"><span class="time">[${String(index).padStart(1, "0")}.${(index + 2)
            .toString()
            .padStart(3, "0")}]</span><span class="msg">${line}</span></div>`
      )
      .join("");
  }

  function render() {
    const gridConnectivity = Number(
      state.stats.grid_connectivity_pct ?? (Number(state.stats.node_health_ratio || 0.99) * 100)
    );
    const pressure = Number(state.stats.pipeline_pressure_bar ?? state.sim.pressure ?? 14.2);
    const vibration = Number(state.stats.vibration_rms ?? state.sim.vibration ?? 0.018);
    const alert = currentAlert();

    setText("gridConn", `${gridConnectivity.toFixed(1)}%`);
    setText("vibVal", vibration.toFixed(3));
    setText("presVal", pressure.toFixed(1));
    setText(
      "anomalyId",
      String(alert?.track_id || alert?.node_id || "PIPELINE-WATCH").toUpperCase()
    );
    setText("ebpfVersion", state.online ? "v2.5.0-AUTH" : "v2.5.0-FALLBACK");

    const alarmTag = document.getElementById("alarmTag");
    if (alarmTag) {
      alarmTag.textContent = alert ? "ALARM" : "WATCH";
      alarmTag.className = `map-tag ${alert ? "tag-alarm" : "tag-stable"}`;
    }

    renderEvents();
    renderSwarm();
    renderKernelLog();
    drawBars();
  }

  function updateEpoch() {
    const now = new Date();
    const text = `${String(now.getHours()).padStart(2, "0")}:${String(
      now.getMinutes()
    ).padStart(2, "0")}:${String(now.getSeconds()).padStart(2, "0")}:${String(
      now.getMilliseconds()
    ).padStart(3, "0")} MS`;
    setText("epochVal", text);
  }

  function assignSnapshot() {
    if (!window.caesarAPI || typeof window.caesarAPI.getState !== "function") {
      return;
    }
    const snapshot = window.caesarAPI.getState();
    state.online = Boolean(snapshot.online);
    state.lastSyncMs = snapshot.lastSuccessMs || state.lastSyncMs;
    state.sim = snapshot.sim || state.sim;
    state.stats = state.stats || snapshot.stats || {};
    state.alerts = state.alerts.length ? state.alerts : snapshot.highInterest || [];
    state.nodeRegistry = state.nodeRegistry?.nodes?.length
      ? state.nodeRegistry
      : snapshot.nodeRegistry || { nodes: [] };
    state.orchestration = state.orchestration || snapshot.orchestration || {};
  }

  window.addEventListener("caesar:stats", (event) => {
    state.stats = event.detail || {};
    render();
  });

  window.addEventListener("caesar:high-interest", (event) => {
    state.alerts = Array.isArray(event.detail) ? event.detail : [];
    render();
  });

  window.addEventListener("caesar:node-registry", (event) => {
    state.nodeRegistry = event.detail || { nodes: [] };
    render();
  });

  window.addEventListener("caesar:orchestration", (event) => {
    state.orchestration = event.detail || {};
    render();
  });

  window.addEventListener("caesar:tick", (event) => {
    state.online = Boolean(event.detail?.online);
    state.lastSyncMs = event.detail?.lastSyncMs || state.lastSyncMs;
    state.sim = event.detail?.state || state.sim;
    render();
  });

  window.addEventListener("resize", resizeCanvases);

  assignSnapshot();
  resizeCanvases();
  render();
  drawInfraMap();
  updateEpoch();
  epochTimer = window.setInterval(updateEpoch, 100);
  window.setInterval(drawBars, 1500);
  window.caesarAPI?.forceRefresh?.();
})();
