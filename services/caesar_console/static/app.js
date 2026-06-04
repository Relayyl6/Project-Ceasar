(function () {
  "use strict";

  if (!document.getElementById("map")) {
    return;
  }

  const state = {
    stats: null,
    latest: {},
    regionalSummary: {},
    learningPlan: {},
    nodeRegistry: { nodes: [] },
    alerts: [],
    governanceAudit: [],
    online: false,
    lastSyncMs: null,
  };

  const MODALITY_ICONS = {
    optical: "videocam",
    thermal: "thermostat",
    radar: "radar",
    manual: "edit_note",
    aggregation: "hub",
  };

  const THREAT_COLORS = {
    "high-interest": "#ff3344",
    monitor: "#ffaa00",
    none: "#00d4ff",
  };

  const ROLE_COLORS = {
    fixed_tower: "#00d4ff",
    relay: "#00ff88",
    regional_hub: "#ffaa00",
  };

  const FALLBACK_NODE_COORDS = {
    "tower-bwari-alpha": [9.335, 7.282],
    "tower-bwari-bravo": [9.248, 7.422],
    "drone-relay-01": [9.302, 7.348],
    "hub-bwari-01": [9.281, 7.382],
    "tower-alpha": [9.335, 7.282],
    "tower-bravo": [9.261, 7.412],
    "tower-gamma": [9.198, 7.298],
    "orch-beta": [9.281, 7.382],
    "relay-01": [9.318, 7.338],
    "relay-02": [9.241, 7.355],
  };

  let map = null;
  let linkLayer = null;
  let markerLayer = null;
  let trackLayer = null;
  let pendingRender = false;

  function getBody(record) {
    return record && record.envelope && record.envelope.body ? record.envelope.body : {};
  }

  function getRecordTimeMs(record) {
    return (
      record?.received_at_ms ??
      record?.timestamp_ms ??
      getBody(record).timestamp_ms ??
      0
    );
  }

  function formatPercent(value, digits = 1) {
    return `${(Math.max(0, Number(value) || 0) * 100).toFixed(digits)}%`;
  }

  function formatNumber(value, digits = 2) {
    return Number(value || 0).toFixed(digits);
  }

  function formatTime(ms) {
    if (!ms) return "--";
    return new Date(ms).toLocaleTimeString([], {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  function formatRelativeTime(ms) {
    if (!ms) return "awaiting sync";
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 5000) return "just now";
    if (delta < 60000) return `${Math.round(delta / 1000)}s ago`;
    if (delta < 3600000) return `${Math.round(delta / 60000)}m ago`;
    return `${Math.round(delta / 3600000)}h ago`;
  }

  function minutesLabel(seconds) {
    return `${Math.max(1, Math.round((Number(seconds) || 0) / 60))}m`;
  }

  function formatLabel(value) {
    return String(value || "none")
      .replaceAll("_", " ")
      .replace(/\b\w/g, (char) => char.toUpperCase());
  }

  function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
  }

  function hexToRgba(hex, alpha) {
    const normalized = String(hex || "#00d4ff").replace("#", "");
    const bigint = Number.parseInt(normalized, 16);
    const red = (bigint >> 16) & 255;
    const green = (bigint >> 8) & 255;
    const blue = bigint & 255;
    return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
  }

  function hashString(input) {
    let hash = 0;
    for (const char of String(input || "")) {
      hash = (hash << 5) - hash + char.charCodeAt(0);
      hash |= 0;
    }
    return Math.abs(hash);
  }

  function getActiveRecords() {
    const cutoff = Number(state.stats?.active_cutoff_ms || 0);
    return Object.values(state.latest || {})
      .filter((record) => getRecordTimeMs(record) >= cutoff && getBody(record).track_id)
      .sort((a, b) => getRecordTimeMs(b) - getRecordTimeMs(a));
  }

  function computeThreatScore(body) {
    const confidence = clamp(Number(body?.confidence || 0), 0, 0.999);
    if (body?.threat_level === "high-interest") {
      return confidence;
    }
    if (body?.threat_level === "monitor") {
      return clamp(confidence * 0.42, 0, 0.999);
    }
    return clamp(confidence * 0.18, 0, 0.999);
  }

  function getFallbackCoord(nodeId, index) {
    const direct = FALLBACK_NODE_COORDS[String(nodeId || "").toLowerCase()];
    if (direct) {
      return direct;
    }
    const seed = hashString(`${nodeId}-${index}`);
    return [
      9.2882 + ((seed % 100) - 50) * 0.0012,
      7.3821 + ((Math.floor(seed / 10) % 100) - 50) * 0.0012,
    ];
  }

  function buildNodeSummaries(activeRecords) {
    const grouped = new Map();
    const registryNodes = state.nodeRegistry?.nodes || [];
    const registryMap = new Map(registryNodes.map((node) => [node.node_id, node]));

    activeRecords.forEach((record) => {
      const body = getBody(record);
      if (!grouped.has(body.node_id)) {
        grouped.set(body.node_id, []);
      }
      grouped.get(body.node_id).push(body);
    });

    const orderedIds = [
      ...new Set([
        ...grouped.keys(),
        ...registryNodes.map((node) => node.node_id),
      ]),
    ];

    return orderedIds.map((nodeId, index) => {
      const registry = registryMap.get(nodeId) || {};
      const tracks = grouped.get(nodeId) || [];
      const latitudes = tracks
        .map((track) => Number(track.geo_latitude))
        .filter(Number.isFinite);
      const longitudes = tracks
        .map((track) => Number(track.geo_longitude))
        .filter(Number.isFinite);
      const fallback = getFallbackCoord(nodeId, index);

      return {
        nodeId,
        role: registry.role || "fixed_tower",
        zone: registry.zone || tracks[0]?.site || "bwari",
        learningLayers: registry.learning_layers || [],
        capabilities: registry.capabilities || [],
        active: tracks.length > 0,
        alertCount: tracks.filter((track) => track.threat_level === "high-interest").length,
        latitude: latitudes.length
          ? latitudes.reduce((sum, value) => sum + value, 0) / latitudes.length
          : fallback[0],
        longitude: longitudes.length
          ? longitudes.reduce((sum, value) => sum + value, 0) / longitudes.length
          : fallback[1],
      };
    });
  }

  function ensureMap() {
    if (map || !window.L) {
      return;
    }

    map = window.L.map("map", {
      zoomControl: false,
      attributionControl: false,
    }).setView([9.2882, 7.3821], 11);

    window.L.tileLayer(
      "https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png",
      { maxZoom: 19 }
    ).addTo(map);

    window.L.control.zoom({ position: "bottomright" }).addTo(map);
    linkLayer = window.L.layerGroup().addTo(map);
    markerLayer = window.L.layerGroup().addTo(map);
    trackLayer = window.L.layerGroup().addTo(map);

    setTimeout(() => map.invalidateSize(), 0);
  }

  function setText(id, value) {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = value;
    }
  }

  function renderConnection() {
    const dot = document.getElementById("connDot");
    const label = document.getElementById("connLabel");
    if (!dot || !label) {
      return;
    }

    if (state.online) {
      dot.style.background = "#00ff88";
      dot.style.boxShadow = "0 0 8px #00ff88";
      label.textContent = `LIVE · SYNCED ${formatRelativeTime(state.lastSyncMs).toUpperCase()}`;
      return;
    }

    dot.style.background = "#ff3344";
    dot.style.boxShadow = "0 0 8px #ff3344";
    label.textContent = state.lastSyncMs
      ? `SIM FALLBACK · LAST LIVE ${formatRelativeTime(state.lastSyncMs).toUpperCase()}`
      : "SIM FALLBACK · WAITING FOR HUB";
  }

  function renderHeader() {
    setText(
      "navRegion",
      String(state.regionalSummary?.region || "Bwari, FCT").toUpperCase()
    );
    setText("hdrNodeHealth", formatPercent(state.stats?.node_health_ratio, 1));
  }

  function renderMetrics(activeRecords) {
    setText("mActiveTracks", String(state.stats?.latest_track_count || activeRecords.length || 0));

    const throughput = document.getElementById("mThroughput");
    if (throughput) {
      throughput.innerHTML = `${formatNumber(
        state.stats?.throughput_events_per_min,
        1
      )} <span class="metric-unit">pkt/min</span>`;
    }

    setText(
      "mThroughputSub",
      `${state.stats?.recent_journal_count || 0} signed envelopes in rolling window`
    );
    setText("mAnomaly", formatPercent(state.stats?.anomaly_probability, 2));
    setText(
      "mAnomalySub",
      `${state.stats?.active_high_interest_count || 0} active high-interest tracks`
    );
    setText("mAlignment", formatNumber(state.stats?.fed_alignment, 2));
    setText(
      "mAlignmentSub",
      `${state.stats?.federated_participant_count || 0}/${state.stats?.registered_node_count || 0} federated participants`
    );
  }

  function renderMap(activeRecords) {
    ensureMap();
    if (!map || !linkLayer || !markerLayer || !trackLayer) {
      return;
    }

    linkLayer.clearLayers();
    markerLayer.clearLayers();
    trackLayer.clearLayers();

    const nodeSummaries = buildNodeSummaries(activeRecords);
    const hub = nodeSummaries.find((node) => node.role === "regional_hub") || nodeSummaries[0];

    nodeSummaries.forEach((node) => {
      const color = node.alertCount > 0 ? "#ff3344" : ROLE_COLORS[node.role] || "#00d4ff";
      const marker = window.L.marker([node.latitude, node.longitude], {
        icon: window.L.divIcon({
          className: "",
          iconSize: [96, 36],
          iconAnchor: [18, 18],
          html: `<div style="display:flex;flex-direction:column;align-items:flex-start;gap:4px">
            <div style="width:14px;height:14px;border-radius:50%;background:${color};box-shadow:0 0 12px ${color};border:2px solid ${color}88"></div>
            <div class="node-marker-label" style="border-color:${color}88;color:${color}">
              ${String(node.nodeId || "").toUpperCase()}
            </div>
          </div>`,
        }),
      });

      marker.bindPopup(
        `<strong>${node.nodeId}</strong><br>Role: ${formatLabel(node.role)}<br>Zone: ${formatLabel(
          node.zone
        )}<br>Status: ${node.active ? "Active" : "Standby"}<br><button onclick="window.focusPipCamera('${node.nodeId}')" style="margin-top:8px; width:100%; padding:6px; background:var(--cyan); color:#000; border:none; font-family:'Share Tech Mono',monospace; cursor:pointer; font-weight:bold;">[ FOCUS LIVE FEED ]</button>`
      );
      markerLayer.addLayer(marker);
    });

    if (hub) {
      nodeSummaries
        .filter((node) => node.nodeId !== hub.nodeId)
        .forEach((node) => {
          linkLayer.addLayer(
            window.L.polyline(
              [
                [hub.latitude, hub.longitude],
                [node.latitude, node.longitude],
              ],
              {
                color: "#00d4ff",
                weight: 1.5,
                opacity: node.active ? 0.8 : 0.35,
                dashArray: "8 6",
              }
            )
          );
        });
    }

    activeRecords.slice(0, 18).forEach((record, index) => {
      const body = getBody(record);
      const fallbackNode = nodeSummaries.find((node) => node.nodeId === body.node_id);
      const baseLat = Number(body.geo_latitude);
      const baseLon = Number(body.geo_longitude);
      const seed = hashString(`${body.track_id}-${index}`);
      const latitude = Number.isFinite(baseLat)
        ? baseLat
        : (fallbackNode?.latitude || 9.2882) + ((seed % 7) - 3) * 0.0015;
      const longitude = Number.isFinite(baseLon)
        ? baseLon
        : (fallbackNode?.longitude || 7.3821) + ((Math.floor(seed / 7) % 7) - 3) * 0.0015;

      trackLayer.addLayer(
        window.L.circleMarker([latitude, longitude], {
          radius: body.threat_level === "high-interest" ? 5 : 4,
          color: THREAT_COLORS[body.threat_level] || "#00d4ff",
          fillColor: THREAT_COLORS[body.threat_level] || "#00d4ff",
          fillOpacity: 0.95,
          weight: 0,
        })
      );
    });

    if (nodeSummaries.length) {
      const bounds = window.L.latLngBounds(
        nodeSummaries.map((node) => [node.latitude, node.longitude])
      );
      if (bounds.isValid()) {
        map.fitBounds(bounds.pad(0.28), { animate: false });
      }
    }

    setText(
      "mapDesc",
      activeRecords.length
        ? `${activeRecords.length} live tracks across ${
            state.stats?.active_node_count || nodeSummaries.filter((node) => node.active).length
          } active nodes. Tracking anomalies in Agricultural (AST) & Tactical domains. Dominant posture: ${formatLabel(
            state.regionalSummary?.dominant_threat_level || "monitor"
          )}.`
        : `No live tracks in the rolling ${minutesLabel(
            state.stats?.activity_window_seconds || 900
          )} window. Swarm routing is idling via ACO/PSO policies while awaiting Edge detection.`
    );
  }

  function renderHeatmap(activeRecords) {
    const grid = document.getElementById("heatGrid");
    if (!grid) {
      return;
    }

    const cells = Array.from({ length: 64 }, () => ({
      intensity: 0,
      color: "#1a2535",
    }));

    const records = activeRecords.length ? activeRecords : Object.values(state.latest || {}).slice(0, 6);
    records.forEach((record, index) => {
      const body = getBody(record);
      const cellIndex = hashString(`${body.track_id}-${body.node_id}-${index}`) % 64;
      const color = THREAT_COLORS[body.threat_level] || "#00d4ff";
      cells[cellIndex].intensity += 1;
      cells[cellIndex].color = color;
      if (cellIndex + 1 < cells.length) {
        cells[cellIndex + 1].intensity += 0.3;
        cells[cellIndex + 1].color = color;
      }
    });

    grid.innerHTML = "";
    cells.forEach((cell) => {
      const el = document.createElement("div");
      el.className = "heat-cell";
      el.style.background = cell.intensity
        ? hexToRgba(cell.color, clamp(cell.intensity * 0.18, 0.18, 0.9))
        : "#1a2535";
      grid.appendChild(el);
    });

    const sigma =
      0.08 +
      Number(state.stats?.anomaly_probability || 0) * 0.8 +
      Math.min(activeRecords.length, 18) * 0.01;
    setText("sigmaVal", `SIGMA: ${sigma.toFixed(3)}`);
    setText(
      "kernelVal",
      `KERNEL: ${activeRecords.length ? "FEDPDM / LIVE" : "PRIOR / STANDBY"}`
    );
  }

  function renderConfidence(activeRecords) {
    const benignRatio = activeRecords.length
      ? activeRecords.filter((record) => getBody(record).threat_level !== "high-interest").length /
        activeRecords.length
      : 1;
    const threatRatio = clamp(Number(state.stats?.anomaly_probability || 0), 0, 1);
    const infraLoad = clamp(Number(state.stats?.node_health_ratio || 0), 0, 1);

    [
      ["confTT", "confTTBar", threatRatio],
      ["confCF", "confCFBar", benignRatio],
      ["confIF", "confIFBar", infraLoad],
    ].forEach(([labelId, barId, value]) => {
      setText(labelId, formatPercent(value, 1));
      const bar = document.getElementById(barId);
      if (bar) {
        bar.style.width = `${Math.round(value * 100)}%`;
      }
    });
  }

  function renderLearning(activeRecords) {
    const segments = document.getElementById("fedSegs");
    if (segments) {
      segments.innerHTML = "";
      const filled = Math.round(clamp(Number(state.stats?.fed_alignment || 0), 0, 1) * 10);
      for (let index = 0; index < 10; index += 1) {
        const segment = document.createElement("div");
        segment.className = "fed-seg";
        segment.style.background = index < filled ? "var(--cyan)" : "var(--surface2)";
        segments.appendChild(segment);
      }
    }

    const roundId = state.learningPlan?.federated_round?.round_id;
    const roundLabel = roundId ? String(roundId).slice(-4) : "--";
    setText(
      "fedRound",
      `Round ${roundLabel} / ${state.stats?.registered_node_count || "--"} nodes`
    );
    setText("fedLoss", `Alignment: ${formatNumber(state.stats?.fed_alignment, 2)}`);

    const rlGrid = document.getElementById("rlGrid");
    if (rlGrid) {
      rlGrid.innerHTML = "";
      const activeNodeIds = new Set(activeRecords.map((record) => getBody(record).node_id));
      (state.nodeRegistry?.nodes || []).slice(0, 6).forEach((node) => {
        const cell = document.createElement("div");
        const hasRl = (node.learning_layers || []).includes("rl");
        const isActive = activeNodeIds.has(node.node_id);
        cell.className = "rl-node";
        cell.style.background = hasRl
          ? "var(--red)"
          : isActive
          ? "var(--cyan)"
          : "var(--surface2)";
        cell.style.color = hasRl ? "#ffffff" : isActive ? "var(--bg)" : "var(--text-dim)";
        cell.textContent = String(node.node_id || "").slice(0, 2).toUpperCase();
        rlGrid.appendChild(cell);
      });
    }

    const rlJobs = (state.learningPlan?.reinforcement_learning || []).length;
    setText(
      "criticStatus",
      rlJobs ? (activeRecords.length ? "ACTIVE" : "READY") : "IDLE"
    );
  }

  function renderTrackLog(activeRecords) {
    setText(
      "trackMeta",
      `Rolling ${minutesLabel(state.stats?.activity_window_seconds || 900)} window`
    );

    const body = document.getElementById("trackBody");
    if (!body) {
      return;
    }

    if (!activeRecords.length) {
      body.innerHTML =
        "<tr><td colspan='6' style='padding:16px 8px;color:var(--text-dim)'>No active tracks in the rolling window.</td></tr>";
      return;
    }

    body.innerHTML = "";
    activeRecords.slice(0, 8).forEach((record) => {
      const payload = getBody(record);
      const row = document.createElement("tr");
      const threatClass =
        payload.threat_level === "high-interest"
          ? "threat-hi"
          : payload.threat_level === "monitor"
          ? "threat-lo"
          : "threat-none";
      const modalities = (payload.contributing_modalities || []).map(
        (modality) =>
          `<span class="material-symbols-outlined mod-icon">${MODALITY_ICONS[modality] || "sensors"}</span>`
      );

      row.innerHTML = `
        <td class="track-id">#${payload.track_id}</td>
        <td>${payload.node_id}</td>
        <td><div class="mod-icons">${modalities.join("") || "--"}</div></td>
        <td class="${threatClass}">${computeThreatScore(payload).toFixed(2)}</td>
        <td>${Number(payload.confidence || 0).toFixed(3)}</td>
        <td style="color:var(--text-dim)">${formatTime(getRecordTimeMs(record))}</td>
      `;
      body.appendChild(row);
    });
  }

  function renderTicker(activeRecords) {
    if (!activeRecords.length) {
      setText(
        "tickerText",
        state.online
          ? `Rolling detection window ${minutesLabel(
              state.stats?.activity_window_seconds || 900
            )} active. Waiting for fresh semantic envelopes from the edge mesh.`
          : "Console is rendering fallback telemetry while the hub reconnects."
      );
      setText("yoloStatus", "> Awaiting fresh EKF frames for YOLO inference...");
      return;
    }

    const latestRecord = getBody(activeRecords[0]);
    setText(
      "tickerText",
      `Node ${latestRecord.node_id} reported ${latestRecord.track_id} at confidence ${Number(
        latestRecord.confidence || 0
      ).toFixed(3)} · ${state.stats?.latest_track_count || activeRecords.length} live tracks · ${
        state.stats?.throughput_events_per_min || 0
      } pkt/min · last sync ${formatRelativeTime(state.lastSyncMs)}`
    );
    
    setText(
      "yoloStatus",
      `> Tracking anomaly: "${latestRecord.threat_level || "unknown"}" [Conf: ${Number(latestRecord.confidence || 0).toFixed(3)}] at Pos [${Number(latestRecord.position_m?.[0]||0).toFixed(1)}, ${Number(latestRecord.position_m?.[1]||0).toFixed(1)}]`
    );
  }

  function renderYoloFeed(activeRecords) {
    const feed = document.getElementById("yoloFeed");
    if (!feed) return;
    // Append new detection lines for the latest two records
    activeRecords.slice(0, 2).forEach((record) => {
      const body = getBody(record);
      if (!body.track_id) return;
      const ts = new Date(getRecordTimeMs(record)).toLocaleTimeString([], {hour12:false, hour:"2-digit", minute:"2-digit", second:"2-digit"});
      const color = body.threat_level === "high-interest" ? "var(--red)" : body.threat_level === "monitor" ? "var(--amber)" : "var(--green)";
      const line = document.createElement("div");
      line.style.color = color;
      line.textContent = `> [${ts}] ${body.track_id} · ${body.threat_level} · conf:${Number(body.confidence||0).toFixed(3)} · ${(body.contributing_modalities||[]).join("+")||"optical"}`;
      feed.appendChild(line);
      // Keep max 30 lines
      while (feed.children.length > 30) feed.removeChild(feed.firstChild);
    });
    feed.scrollTop = feed.scrollHeight;

    // Update yoloStatus with the most recent detection
    const yoloStatus = document.getElementById("yoloStatus");
    if (yoloStatus && activeRecords.length) {
      const latest = getBody(activeRecords[0]);
      yoloStatus.textContent = `> Tracking: "${latest.class_label || latest.threat_level}" [Conf: ${Number(latest.confidence||0).toFixed(3)}] at [${Number(latest.position_m?.[0]||0).toFixed(1)}, ${Number(latest.position_m?.[1]||0).toFixed(1)}]`;
    } else if (yoloStatus) {
      yoloStatus.textContent = "> Awaiting fresh EKF frames for YOLO inference...";
    }
  }

  function renderAnomalyLog(activeRecords) {
    const log = document.getElementById("anomalyLog");
    if (!log) return;
    // Append high-interest detections as they arrive
    const hiRecords = activeRecords.filter(r => getBody(r).threat_level === "high-interest");
    hiRecords.forEach((record) => {
      const body = getBody(record);
      if (!body.track_id) return;
      const ts = new Date(getRecordTimeMs(record)).toLocaleTimeString([], {hour12:false, hour:"2-digit", minute:"2-digit", second:"2-digit"});
      // Avoid exact duplicate lines
      const lineText = `[${ts}] ⚠ ${body.track_id} · ${body.node_id} · conf:${Number(body.confidence||0).toFixed(3)}`;
      const existing = Array.from(log.children).map(el => el.textContent);
      if (!existing.includes(lineText)) {
        const el = document.createElement("div");
        el.style.cssText = "color:var(--red);border-bottom:1px solid rgba(255,51,68,0.1);padding:2px 0;";
        el.textContent = lineText;
        log.prepend(el);
        while (log.children.length > 25) log.removeChild(log.lastChild);
      }
    });
    // If no real data yet, show waiting state (but not static placeholder text)
    if (log.children.length === 0) {
      const el = document.createElement("div");
      el.style.color = "var(--text-dim)";
      el.textContent = `> Monitoring zone... ${state.online ? "LIVE" : "SIM"} · ${new Date().toLocaleTimeString()}`;
      log.appendChild(el);
      setTimeout(() => { if (el.parentNode) el.parentNode.removeChild(el); }, 3000);
    }
  }

  function renderGovernanceLog() {
    const log = document.getElementById("governanceLog");
    if (!log) return;
    const audits = state.governanceAudit || [];
    if (!audits.length) {
      log.textContent = "> Governance audit stream: awaiting records...";
      return;
    }
    log.innerHTML = "";
    audits.slice(0, 8).forEach((entry) => {
      const ts = entry.timestamp_ms ? new Date(entry.timestamp_ms).toLocaleTimeString([], {hour12:false, hour:"2-digit", minute:"2-digit", second:"2-digit"}) : "--:--:--";
      const nodes = entry.regional_summary?.active_nodes ?? "?";
      const tracks = entry.regional_summary?.active_tracks ?? "?";
      const threat = entry.regional_summary?.dominant_threat_level ?? "none";
      const color = threat === "high-interest" ? "var(--red)" : threat === "monitor" ? "var(--amber)" : "var(--text-dim)";
      const el = document.createElement("div");
      el.style.cssText = `color:${color};border-bottom:1px solid rgba(255,255,255,0.04);padding:1px 0;`;
      el.textContent = `[${ts}] nodes:${nodes} tracks:${tracks} threat:${threat}`;
      log.appendChild(el);
    });
  }

  function renderAll() {
    pendingRender = false;
    const activeRecords = getActiveRecords();
    renderConnection();
    renderHeader();
    renderMetrics(activeRecords);
    renderMap(activeRecords);
    renderHeatmap(activeRecords);
    renderConfidence(activeRecords);
    renderLearning(activeRecords);
    renderTrackLog(activeRecords);
    renderTicker(activeRecords);
    renderYoloFeed(activeRecords);
    renderAnomalyLog(activeRecords);
    renderGovernanceLog();
    updatePipCamera(activeRecords);
  }

  function scheduleRender() {
    if (pendingRender) {
      return;
    }
    pendingRender = true;
    window.requestAnimationFrame(renderAll);
  }

  function assignSnapshot() {
    if (!window.caesarAPI || typeof window.caesarAPI.getState !== "function") {
      return;
    }
    const snapshot = window.caesarAPI.getState();
    state.stats = state.stats || snapshot.stats;
    state.latest = snapshot.latest || state.latest;
    state.regionalSummary = snapshot.regionalSummary || state.regionalSummary;
    state.learningPlan = snapshot.learningPlan || state.learningPlan;
    state.nodeRegistry = snapshot.nodeRegistry || state.nodeRegistry;
    state.alerts = snapshot.highInterest || state.alerts;
    state.online = Boolean(snapshot.online);
    state.lastSyncMs = snapshot.lastSuccessMs || state.lastSyncMs;
  }

  // PIP Camera Auto-Switching Logic
  let activeCameraNode = null;

  function updatePipCamera(activeRecords) {
    const pip = document.getElementById("cameraPip");
    if (!pip) return;

    // Find the most recent high-interest track
    const hiTracks = activeRecords.filter(r => getBody(r).threat_level === "high-interest");
    const targetRecord = hiTracks.length > 0 ? hiTracks[0] : (activeRecords.length > 0 ? activeRecords[0] : null);

    if (!targetRecord) {
      document.getElementById("camNodeId").textContent = "IDLE";
      document.getElementById("camTargetLabel").textContent = "scanning...";
      document.getElementById("camTargetBox").style.borderColor = "var(--cyan)";
      document.getElementById("camStream").style.display = "none";
      document.getElementById("camStreamFallback").style.display = "flex";
      return;
    }

    const t = getBody(targetRecord);
    const targetNode = t.node_id;

    // Switch stream if node changed
    if (activeCameraNode !== targetNode) {
      activeCameraNode = targetNode;
      document.getElementById("camNodeId").textContent = targetNode;
      const streamImg = document.getElementById("camStream");
      const fallback  = document.getElementById("camStreamFallback");
      
      if (streamImg) {
        streamImg.style.display = "block";
        if (fallback) fallback.style.display = "none";
        // To avoid endless reloading in sim, we use a static fallback if it's a simulated node without a real server,
        // but for this UI purpose, we just point it to the node stream URL.
        streamImg.src = `/api/camera-stream?node=${encodeURIComponent(targetNode)}&_=${Date.now()}`;
      }
    }

    // Animate bounding box
    const box = document.getElementById("camTargetBox");
    const label = document.getElementById("camTargetLabel");

    if (box) {
      // Jitter box slightly for "live" feel
      const top = 30 + (Math.random() * 20);
      const left = 25 + (Math.random() * 30);
      box.style.top = `${top}%`;
      box.style.left = `${left}%`;
      box.style.borderColor = THREAT_COLORS[t.threat_level] || "var(--cyan)";
    }

    if (label) {
      label.textContent = `${t.threat_level} [${Number(t.confidence||0).toFixed(2)}]`;
    }
  }

  // Allow map marker popups to manually pin the PIP camera to a specific node.
  // Once pinned, the auto-switching pauses for 30s before resuming.
  let _pipPinTimeout = null;
  window.focusPipCamera = function(nodeId) {
    if (map) map.closePopup();
    // Override activeCameraNode so the next updatePipCamera call switches to this node
    activeCameraNode = null; // force a stream reload
    const streamImg = document.getElementById("camStream");
    const fallback  = document.getElementById("camStreamFallback");
    const nodeLabel = document.getElementById("camNodeId");
    if (nodeLabel) nodeLabel.textContent = nodeId;
    if (streamImg) {
      streamImg.style.display = "block";
      if (fallback) fallback.style.display = "none";
      streamImg.src = `/api/camera-stream?node=${encodeURIComponent(nodeId)}&_=${Date.now()}`;
    }
    activeCameraNode = nodeId;
    // Pause auto-switching for 30s so the operator can observe this node
    clearTimeout(_pipPinTimeout);
    _pipPinTimeout = setTimeout(() => { activeCameraNode = null; }, 30000);
  };

  window.openCamera = function(nodeId) {
    const modal = document.getElementById("cameraModal");
    const modalTitle = document.getElementById("modalNodeId");
    const modalImg = document.getElementById("modalCamStream");
    const fallback = document.getElementById("modalCamStreamFallback");
    
    if (modal) {
      modal.style.display = "flex";
    }
    if (modalTitle) {
      modalTitle.textContent = `${nodeId} · DIRECT OPTICAL TRANSMISSION`;
    }
    if (modalImg) {
      modalImg.style.display = "block";
      if (fallback) fallback.style.display = "none";
      modalImg.src = `/api/camera-stream?node=${encodeURIComponent(nodeId)}&_=${Date.now()}`;
    }
  };

  window.closeCamera = function() {
    const modal = document.getElementById("cameraModal");
    const modalImg = document.getElementById("modalCamStream");
    if (modal) {
      modal.style.display = "none";
    }
    if (modalImg) {
      modalImg.src = "";
    }
  };

  window.addEventListener("caesar:stats", (event) => {
    state.stats = event.detail || {};
    scheduleRender();
  });

  window.addEventListener("caesar:latest", (event) => {
    state.latest = event.detail || {};
    scheduleRender();
  });

  window.addEventListener("caesar:regional-summary", (event) => {
    state.regionalSummary = event.detail || {};
    scheduleRender();
  });

  window.addEventListener("caesar:learning-plan", (event) => {
    state.learningPlan = event.detail || {};
    scheduleRender();
  });

  window.addEventListener("caesar:node-registry", (event) => {
    state.nodeRegistry = event.detail || { nodes: [] };
    scheduleRender();
  });

  window.addEventListener("caesar:high-interest", (event) => {
    state.alerts = Array.isArray(event.detail) ? event.detail : [];
    scheduleRender();
  });

  window.addEventListener("caesar:governance-audit", (event) => {
    state.governanceAudit = Array.isArray(event.detail) ? event.detail : [];
    scheduleRender();
  });

  window.addEventListener("caesar:tick", (event) => {
    state.online = Boolean(event.detail?.online);
    state.lastSyncMs = event.detail?.lastSyncMs || state.lastSyncMs;
    scheduleRender();
  });

  assignSnapshot();
  scheduleRender();
  window.caesarAPI?.forceRefresh?.();
})();
