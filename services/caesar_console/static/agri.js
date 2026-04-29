(function () {
  "use strict";

  const canvas = document.getElementById("yieldCanvas");
  if (!canvas) {
    return;
  }

  const ctx = canvas.getContext("2d");
  const state = {
    stats: {},
    regionalSummary: {},
    learningPlan: {},
    online: false,
    lastSyncMs: null,
    sim: {},
  };

  let humidityBaseline = null;
  let frame = 0;

  function formatNumber(value, digits = 2) {
    return Number(value || 0).toFixed(digits);
  }

  function formatPercent(value, digits = 1) {
    return `${Number(value || 0).toFixed(digits)}%`;
  }

  function dominantSite() {
    const entries = Object.entries(state.regionalSummary?.site_activity || {});
    if (!entries.length) {
      return "bwari-central";
    }
    entries.sort((left, right) => right[1] - left[1]);
    return entries[0][0];
  }

  function resizeCanvas() {
    canvas.width = canvas.offsetWidth;
    canvas.height = canvas.offsetHeight;
  }

  function drawYieldSurface() {
    if (canvas.width !== canvas.offsetWidth || canvas.height !== canvas.offsetHeight) {
      resizeCanvas();
    }

    const width = canvas.width;
    const height = canvas.height;
    const soil = Number(state.stats.soil_moisture_pct ?? state.sim.soilMoisture ?? 62);
    const ndvi = Number(state.stats.ndvi_index ?? state.sim.ndvi ?? 0.82);
    const humidity = Number(state.stats.humidity_pct ?? state.sim.humidity ?? 84);
    const anomaly = Number(state.stats.anomaly_probability ?? state.sim.anomalyProb ?? 0.04);

    ctx.clearRect(0, 0, width, height);
    ctx.fillStyle = "#0c1a10";
    ctx.fillRect(0, 0, width, height);

    const bands = 15;
    for (let band = 0; band < bands; band += 1) {
      const progress = band / bands;
      const hue = 110 + progress * 70 + ndvi * 24;
      const opacity = 0.12 + progress * 0.28;
      const amplitude = height * (0.06 + anomaly * 0.32 + (1 - ndvi) * 0.06);
      ctx.strokeStyle = `hsla(${hue}, 72%, ${36 + soil * 0.14}%, ${opacity})`;
      ctx.lineWidth = 0.9;
      ctx.beginPath();

      for (let x = 0; x <= width; x += 3) {
        const nx = x / Math.max(width, 1);
        const wave =
          Math.sin(nx * 4.5 + frame * 0.03 + band * 0.45) * 0.4 +
          Math.sin(nx * 9.3 - frame * 0.018 + band * 0.7) * 0.2 +
          Math.sin(nx * 15.1 + frame * 0.015) * 0.08;
        const y =
          height * (0.16 + progress * 0.68) -
          wave * amplitude -
          (humidity - 70) * 0.05;
        if (x === 0) {
          ctx.moveTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.stroke();
    }

    const hotspotX = width * (0.45 + anomaly * 0.4);
    const hotspotY = height * (0.52 - ndvi * 0.12);
    const glow = ctx.createRadialGradient(hotspotX, hotspotY, 0, hotspotX, hotspotY, 70);
    glow.addColorStop(0, `rgba(255, 68, 85, ${0.08 + anomaly * 0.8})`);
    glow.addColorStop(1, "rgba(255, 68, 85, 0)");
    ctx.fillStyle = glow;
    ctx.beginPath();
    ctx.arc(hotspotX, hotspotY, 70, 0, Math.PI * 2);
    ctx.fill();

    frame += 1;
    window.requestAnimationFrame(drawYieldSurface);
  }

  function renderYieldBars() {
    const container = document.getElementById("yieldBars");
    if (!container) {
      return;
    }

    const soil = Number(state.stats.soil_moisture_pct ?? state.sim.soilMoisture ?? 62) / 100;
    const ndvi = Number(state.stats.ndvi_index ?? state.sim.ndvi ?? 0.82);
    const humidity = Number(state.stats.humidity_pct ?? state.sim.humidity ?? 84) / 100;
    const efficiency = Number(state.stats.ast_efficiency_pct ?? state.sim.astEff ?? 90) / 100;
    const alignment = Number(state.stats.fed_alignment ?? state.sim.alignment ?? 0.88);

    container.innerHTML = "";
    [
      { value: soil, color: "#00d4c8" },
      { value: ndvi, color: "#00ff88" },
      { value: humidity, color: "#ffcc00" },
      { value: efficiency, color: "#00d4c8" },
      { value: alignment, color: "#00ff88" },
    ].forEach((entry) => {
      const bar = document.createElement("div");
      bar.className = "ybar";
      bar.style.width = `${Math.round(entry.value * 64)}px`;
      bar.style.background = entry.color;
      container.appendChild(bar);
    });
  }

  function setText(id, value) {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = value;
    }
  }

  function render() {
    const integrity = Number(
      state.stats.system_integrity_pct ?? 98.5 + Number(state.stats.fed_alignment ?? 0.88) * 1.3
    );
    const soil = Number(state.stats.soil_moisture_pct ?? state.sim.soilMoisture ?? 62.4);
    const ndvi = Number(state.stats.ndvi_index ?? state.sim.ndvi ?? 0.82);
    const humidity = Number(state.stats.humidity_pct ?? state.sim.humidity ?? 84.1);
    const astEfficiency = Number(state.stats.ast_efficiency_pct ?? state.sim.astEff ?? 92);
    const waterSavings = Math.round(
      state.stats.water_savings_l_wk ?? state.sim.waterSavings ?? 12400
    );
    const yieldProjection = Number(
      state.stats.max_yield_t_ha ?? 7.5 + soil * 0.02 + ndvi * 1.2
    );
    const swarmUnits = Math.round(state.stats.swarm_units ?? state.sim.swarmUnits ?? 128);
    const vtolBattery = Number(state.stats.vtol_battery_pct ?? state.sim.vtolBat ?? 88);
    const fedRound = state.learningPlan?.federated_round?.round_id;
    const fedLoss = Number(state.sim.fedLoss ?? Math.max(0.0001, 1 - Number(state.stats.fed_alignment || 0.88)));
    const fedAlignment = Number(state.stats.fed_alignment ?? state.sim.alignment ?? 0.88);
    const participantCount = state.stats.federated_participant_count || 0;
    const registeredCount = state.stats.registered_node_count || 0;

    if (humidityBaseline === null) {
      humidityBaseline = humidity;
    }

    const heroStatus = document.getElementById("heroStatus");
    if (heroStatus) {
      heroStatus.textContent = state.online ? "LIVE FEED" : "FALLBACK MODE";
      heroStatus.style.color = state.online ? "var(--green)" : "var(--amber)";
    }

    setText("heroIntegrity", `${integrity.toFixed(2)}%`);
    setText("heroSwarm", `ACTIVE: ${swarmUnits}`);
    setText("soilMoisture", soil.toFixed(1));
    setText("ndviVal", ndvi.toFixed(2));
    setText("ndviOverlap", `${Math.round(ndvi * 100)}% OVERLAP`);
    setText("humidVal", humidity.toFixed(1));
    setText(
      "humidDelta",
      `${humidity >= humidityBaseline ? "+" : ""}${(humidity - humidityBaseline).toFixed(1)}%`
    );
    setText("maxYield", `${yieldProjection.toFixed(2)} t/ha`);
    setText("astZone", String(dominantSite()).replaceAll("-", " ").toUpperCase());
    setText("waterSavings", `+${waterSavings.toLocaleString()}L / WK`);
    setText("astEff", `${Math.round(astEfficiency)}%`);
    setText("fedEpoch", fedRound ? String(fedRound).slice(-4) : "--");
    setText("fedLossVal", fedLoss.toFixed(4));
    setText("fedConv", fedAlignment.toFixed(3));
    setText("fedConfidence", fedAlignment.toFixed(3));
    setText("fedNodes", `${participantCount} / ${registeredCount || participantCount}`);
    setText("vBat1Val", `${Math.round(vtolBattery)}%`);
    setText(
      "vSig2",
      `${-38 - Math.round((Number(state.stats.anomaly_probability || 0.04) * 100) / 2)}dBm`
    );

    const soilBar = document.getElementById("soilBar");
    if (soilBar) soilBar.style.width = `${soil}%`;
    const ndviBar = document.getElementById("ndviBar");
    if (ndviBar) ndviBar.style.width = `${ndvi * 100}%`;
    const humidBar = document.getElementById("humidBar");
    if (humidBar) humidBar.style.width = `${humidity}%`;
    const astEffBar = document.getElementById("astEffBar");
    if (astEffBar) astEffBar.style.width = `${astEfficiency}%`;
    const vBat1 = document.getElementById("vBat1");
    if (vBat1) vBat1.style.width = `${vtolBattery}%`;

    renderYieldBars();
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
    state.regionalSummary = state.regionalSummary || snapshot.regionalSummary || {};
    state.learningPlan = state.learningPlan || snapshot.learningPlan || {};
  }

  window.addEventListener("caesar:stats", (event) => {
    state.stats = event.detail || {};
    render();
  });

  window.addEventListener("caesar:regional-summary", (event) => {
    state.regionalSummary = event.detail || {};
    render();
  });

  window.addEventListener("caesar:learning-plan", (event) => {
    state.learningPlan = event.detail || {};
    render();
  });

  window.addEventListener("caesar:tick", (event) => {
    state.online = Boolean(event.detail?.online);
    state.lastSyncMs = event.detail?.lastSyncMs || state.lastSyncMs;
    state.sim = event.detail?.state || state.sim;
    render();
  });

  window.addEventListener("resize", resizeCanvas);

  assignSnapshot();
  resizeCanvas();
  render();
  drawYieldSurface();
  window.caesarAPI?.forceRefresh?.();
})();
