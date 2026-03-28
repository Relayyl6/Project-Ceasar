// ─── Constants ───────────────────────────────────────────────────────────────

const MODALITY_ICONS = {
  optical: "videocam",
  thermal: "thermostat",
  radar: "radar",
  manual: "edit_note",
};

const ROLE_COLORS = {
  fixed_tower: "#50e1f9",
  relay: "#9df197",
  regional_hub: "#ffd16c",
};

const THREAT_COLORS = {
  "high-interest": "#ff716c",
  monitor: "#ffd16c",
  none: "#7d8694",
};

// ─── Connection State ─────────────────────────────────────────────────────────

const connState = {
  online: false,
  consecutiveFailures: 0,
  lastSuccessMs: null,
  lastAttemptMs: null,
  retryDelayMs: 2000,
  maxRetryDelayMs: 30000,
  endpointHealth: {},
  refreshTimer: null,
  refreshIntervalMs: 8000,
  isRefreshing: false,
};

// ─── Utility ──────────────────────────────────────────────────────────────────

function getBody(record) {
  return record?.envelope?.body ?? {};
}

function getRecordTimeMs(record) {
  return record?.received_at_ms ?? record?.timestamp_ms ?? getBody(record)?.timestamp_ms ?? 0;
}

function formatLabel(label) {
  return String(label ?? "")
    .replaceAll("_", " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
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
  if (!ms) return "never";
  const delta = Date.now() - ms;
  if (delta < 5000) return "just now";
  if (delta < 60000) return `${Math.round(delta / 1000)}s ago`;
  if (delta < 3600000) return `${Math.round(delta / 60000)}m ago`;
  return `${Math.round(delta / 3600000)}h ago`;
}

function minutesLabel(seconds) {
  const minutes = Math.max(1, Math.round((seconds || 0) / 60));
  return `${minutes}m`;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function hashString(input) {
  let hash = 0;
  for (const char of String(input)) {
    hash = (hash << 5) - hash + char.charCodeAt(0);
    hash |= 0;
  }
  return Math.abs(hash);
}

function hexToRgba(hex, alpha) {
  const normalized = hex.replace("#", "");
  const bigint = parseInt(normalized, 16);
  const r = (bigint >> 16) & 255;
  const g = (bigint >> 8) & 255;
  const b = bigint & 255;
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function shortenNodeLabel(nodeId) {
  const parts = String(nodeId || "").split("-");
  if (parts.length <= 2) return String(nodeId || "").toUpperCase();
  return `${parts[0]}-${parts[parts.length - 1]}`.toUpperCase();
}

function badgeClassForThreat(threat) {
  if (threat === "high-interest") return "data-pill data-pill-error";
  if (threat === "monitor") return "data-pill data-pill-tertiary";
  return "data-pill data-pill-primary";
}

function getActiveRecords(latest, cutoffMs) {
  return Object.values(latest || {})
    .filter((r) => getRecordTimeMs(r) >= cutoffMs && getBody(r).track_id)
    .sort((a, b) => getRecordTimeMs(b) - getRecordTimeMs(a));
}

function computeThreatScore(body) {
  const base = Number(body.confidence || 0);
  if (body.threat_level === "high-interest") return clamp(base, 0, 0.999);
  if (body.threat_level === "monitor") return clamp(base * 0.42, 0, 0.999);
  return clamp(base * 0.18, 0, 0.999);
}

// ─── Fetch with per-endpoint health tracking ──────────────────────────────────

async function loadJson(path, fallback = null) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 7000);
  try {
    const response = await fetch(path, { cache: "no-store", signal: controller.signal });
    clearTimeout(timeout);
    if (!response.ok) {
      connState.endpointHealth[path] = response.status === 404 ? "missing" : "error";
      console.warn(`[caesar] ${path} → HTTP ${response.status}`);
      return fallback;
    }
    const data = await response.json();
    connState.endpointHealth[path] = "ok";
    return data;
  } catch (err) {
    clearTimeout(timeout);
    connState.endpointHealth[path] = err.name === "AbortError" ? "timeout" : "error";
    console.warn(`[caesar] ${path} → ${err.message}`);
    return fallback;
  }
}

// ─── Connection & Status UI ───────────────────────────────────────────────────

function updateConnectionBadge() {
  const dot = document.getElementById("connDot");
  const label = document.getElementById("connLabel");
  const spinner = document.getElementById("refreshingSpinner");
  const retryBtn = document.getElementById("retryBtn");
  const badge = document.getElementById("connBadge");

  if (!dot) return;

  if (connState.isRefreshing) {
    spinner?.classList.remove("hidden");
  } else {
    spinner?.classList.add("hidden");
  }

  if (connState.online) {
    dot.style.background = "#9df197";
    dot.style.boxShadow = "0 0 8px #9df197";
    label.textContent = connState.lastSuccessMs
      ? `Live · synced ${formatRelativeTime(connState.lastSuccessMs)}`
      : "Live";
    if (badge) badge.style.borderColor = "rgba(157,241,151,0.25)";
    retryBtn?.classList.add("hidden");
  } else if (connState.consecutiveFailures > 0) {
    dot.style.background = "#ff716c";
    dot.style.boxShadow = "0 0 8px #ff716c";
    label.textContent = connState.lastSuccessMs
      ? `Offline · last sync ${formatRelativeTime(connState.lastSuccessMs)}`
      : `Offline · ${connState.consecutiveFailures} failure${connState.consecutiveFailures !== 1 ? "s" : ""}`;
    if (badge) badge.style.borderColor = "rgba(255,113,108,0.25)";
    retryBtn?.classList.remove("hidden");
  } else {
    dot.style.background = "#ffd16c";
    dot.style.boxShadow = "0 0 8px #ffd16c";
    label.textContent = "Connecting…";
    if (badge) badge.style.borderColor = "rgba(255,209,108,0.25)";
    retryBtn?.classList.add("hidden");
  }
}

function updateEndpointHealthPanel() {
  const panel = document.getElementById("endpointHealthPanel");
  if (!panel) return;
  const entries = Object.entries(connState.endpointHealth);
  if (!entries.length) {
    panel.innerHTML = `<span class="text-[#7d8694] font-mono text-[0.6rem]">No requests yet</span>`;
    return;
  }
  panel.innerHTML = entries.map(([path, status]) => {
    const short = path.replace(/^\/api\//, "").replace(/\?.*$/, "").replace(/^\//, "");
    const color = status === "ok" ? "#9df197" : status === "missing" ? "#ffd16c" : "#ff716c";
    const icon = status === "ok" ? "check_circle" : status === "missing" ? "help" : "error";
    return `<div class="flex items-center gap-1.5 min-w-0">
      <span class="material-symbols-outlined flex-shrink-0" style="font-size:0.75rem;color:${color};font-variation-settings:'FILL' 1">${icon}</span>
      <span class="font-mono text-[0.6rem] uppercase truncate" style="color:${color}">${short}</span>
    </div>`;
  }).join("");
}

function renderDataStalenessWarning(stats) {
  const el = document.getElementById("stalenessWarning");
  if (!el) return;
  if (stats && stats.stale) {
    el.classList.remove("hidden");
    const span = document.getElementById("stalenessText");
    if (span) span.textContent = `⚠  Data pipeline stale — no detections inside the rolling ${minutesLabel(stats.activity_window_seconds)} window. Output files may be empty or not yet written by the edge pipeline.`;
  } else {
    el.classList.add("hidden");
  }
}

function renderOfflineBanner(show) {
  const el = document.getElementById("offlineBanner");
  if (!el) return;
  if (show) el.classList.remove("hidden");
  else el.classList.add("hidden");
}

// ─── Mission logic ────────────────────────────────────────────────────────────

function determineMissionMode(stats, regionalSummary) {
  if ((stats.active_high_interest_count || 0) > 0 || regionalSummary.dominant_threat_level === "high-interest") {
    return "tactical";
  }
  const siteText = JSON.stringify(regionalSummary.site_activity || {}).toLowerCase();
  if (siteText.includes("farm") || siteText.includes("field") || siteText.includes("irrigation")) {
    return "agriculture";
  }
  if ((stats.active_node_count || 0) > 0) return "surveillance";
  return "infrastructure";
}

function setMissionMode(mode) {
  const map = {
    agriculture: "modeAgriculture",
    infrastructure: "modeInfrastructure",
    tactical: "modeTactical",
    surveillance: "modeSurveillance",
  };
  for (const id of Object.values(map)) {
    document.getElementById(id)?.classList.remove("mission-link-active");
  }
  document.getElementById(map[mode] || map.tactical)?.classList.add("mission-link-active");
}

function buildMissionNarrative(stats, regionalSummary, learningPlan, nodeRegistry) {
  const activeTracks = stats.latest_track_count || 0;
  const activeNodes = stats.active_node_count || 0;
  const registeredNodes = stats.registered_node_count || 0;
  const highInterest = stats.active_high_interest_count || 0;
  const supervisedJobs = (learningPlan.supervised_learning || []).length;
  const rlJobs = (learningPlan.reinforcement_learning || []).length;

  if (!activeTracks) {
    return `The command layer is monitoring ${registeredNodes} registered nodes across ${
      regionalSummary.region || "the region"
    }. No active tracks are inside the rolling ${minutesLabel(
      stats.activity_window_seconds
    )} window, so the console is emphasizing registry posture, orchestration policy, and learning readiness instead of stale totals.`;
  }

  return `The mesh is coordinating ${activeTracks} active tracks across ${activeNodes} live nodes in ${
    regionalSummary.region || "the region"
  }. ${highInterest} high-interest track${
    highInterest === 1 ? "" : "s"
  } currently shape routing priority, while ${supervisedJobs} supervised and ${rlJobs} reinforcement jobs keep the learning fabric aligned with mission demand.`;
}

// ─── Renderers ────────────────────────────────────────────────────────────────

function updateHeader(stats, regionalSummary, nodeRegistry) {
  const region = regionalSummary.region || "Bwari, FCT";
  document.getElementById("regionLabel").textContent = String(region).toUpperCase();
  document.getElementById("nodeHealthLabel").textContent = `Node Health: ${formatPercent(stats.node_health_ratio, 1)}`;
  document.getElementById("windowLabel").textContent = `Rolling Window: ${minutesLabel(stats.activity_window_seconds)}`;
  document.getElementById("clusterLabel").textContent = (nodeRegistry.cluster_id || "uriel orchestrator").toUpperCase();
}

function renderMetricCards(stats) {
  document.getElementById("metricActiveTracks").textContent = stats.latest_track_count || 0;
  document.getElementById("metricThroughput").innerHTML = `${formatNumber(stats.throughput_events_per_min, 1)} <span class="text-base font-normal">pkt/min</span>`;
  document.getElementById("metricAnomaly").textContent = formatPercent(stats.anomaly_probability, 2);
  document.getElementById("metricAlignment").textContent = formatNumber(stats.fed_alignment, 2);
  document.getElementById("metricTrackSubtext").textContent = `Active tracks inside rolling ${minutesLabel(stats.activity_window_seconds)} window`;
  document.getElementById("metricThroughputSubtext").textContent = `${stats.recent_journal_count || 0} semantic envelopes observed recently`;
  document.getElementById("metricAnomalySubtext").textContent = `${stats.active_high_interest_count || 0} active high-interest tracks`;
  document.getElementById("metricAlignmentSubtext").textContent = `${stats.federated_participant_count || 0}/${stats.registered_node_count || 0} registered nodes in round`;
}

function buildNodeSummaries(activeRecords, nodeRegistry) {
  const registryMap = new Map((nodeRegistry.nodes || []).map((n) => [n.node_id, n]));
  const groups = new Map();
  for (const record of activeRecords) {
    const body = getBody(record);
    if (!groups.has(body.node_id)) groups.set(body.node_id, []);
    groups.get(body.node_id).push(body);
  }

  const summaries = [];
  for (const [nodeId, tracks] of groups.entries()) {
    const reg = registryMap.get(nodeId) || {};
    const lats = tracks.map((t) => Number(t.geo_latitude)).filter(Number.isFinite);
    const lons = tracks.map((t) => Number(t.geo_longitude)).filter(Number.isFinite);
    summaries.push({
      nodeId, role: reg.role || "fixed_tower", zone: reg.zone || tracks[0]?.site || "unknown",
      protocols: reg.protocols || [], learningLayers: reg.learning_layers || [],
      capabilities: reg.capabilities || [], tracks,
      alertCount: tracks.filter((t) => t.threat_level === "high-interest").length,
      latitude: lats.length ? lats.reduce((s, v) => s + v, 0) / lats.length : null,
      longitude: lons.length ? lons.reduce((s, v) => s + v, 0) / lons.length : null,
      active: true,
    });
  }

  if (!summaries.length) {
    return (nodeRegistry.nodes || []).map((node, i) => ({
      nodeId: node.node_id, role: node.role, zone: node.zone,
      protocols: node.protocols || [], learningLayers: node.learning_layers || [],
      capabilities: node.capabilities || [], tracks: [], alertCount: 0,
      latitude: null, longitude: null, active: false, fallbackIndex: i,
    }));
  }

  for (const node of nodeRegistry.nodes || []) {
    if (!groups.has(node.node_id)) {
      summaries.push({
        nodeId: node.node_id, role: node.role, zone: node.zone,
        protocols: node.protocols || [], learningLayers: node.learning_layers || [],
        capabilities: node.capabilities || [], tracks: [], alertCount: 0,
        latitude: null, longitude: null, active: false,
      });
    }
  }
  return summaries;
}

function fallbackNodePosition(summary, index, total) {
  const hash = hashString(summary.nodeId);
  if (summary.role === "regional_hub") return { x: 74, y: 68 };
  if (summary.role === "relay") return { x: 52 + (hash % 16), y: 26 + (hash % 10) };
  const cols = Math.max(2, Math.ceil(Math.sqrt(total || 1)));
  return {
    x: 20 + (index % cols) * (52 / Math.max(cols - 1, 1)) + (hash % 7),
    y: 28 + Math.floor(index / cols) * 18 + (hash % 6),
  };
}

function buildPositionResolver(nodeSummaries) {
  const geoNodes = nodeSummaries.filter((s) => Number.isFinite(s.latitude) && Number.isFinite(s.longitude));
  if (geoNodes.length >= 2) {
    const lats = geoNodes.map((n) => n.latitude);
    const lons = geoNodes.map((n) => n.longitude);
    const [minLat, maxLat, minLon, maxLon] = [Math.min(...lats), Math.max(...lats), Math.min(...lons), Math.max(...lons)];
    const [latSpan, lonSpan] = [maxLat - minLat, maxLon - minLon];
    if (latSpan > 0.00001 || lonSpan > 0.00001) {
      return (summary, index) => {
        if (!Number.isFinite(summary.latitude) || !Number.isFinite(summary.longitude)) {
          return fallbackNodePosition(summary, index, nodeSummaries.length);
        }
        return {
          x: 12 + ((summary.longitude - minLon) / Math.max(lonSpan, 0.00001)) * 76,
          y: 14 + (1 - (summary.latitude - minLat) / Math.max(latSpan, 0.00001)) * 70,
        };
      };
    }
  }
  return (summary, index) => fallbackNodePosition(summary, index, nodeSummaries.length);
}

function renderMap(stats, regionalSummary, activeRecords, nodeRegistry) {
  const markerLayer = document.getElementById("mapMarkers");
  const linkLayer = document.getElementById("mapLinks");
  markerLayer.innerHTML = "";
  linkLayer.innerHTML = "";

  const nodeSummaries = buildNodeSummaries(activeRecords, nodeRegistry);
  const resolvePosition = buildPositionResolver(nodeSummaries);
  const positions = new Map();

  document.getElementById("mapTitle").textContent = String(regionalSummary.region || "Regional Theater").toUpperCase();
  document.getElementById("mapNarrative").textContent =
    stats.latest_track_count > 0
      ? `${stats.latest_track_count} live tracks across ${stats.active_node_count} active nodes. Dominant posture: ${formatLabel(
          regionalSummary.dominant_threat_level || "none"
        )}, rolling ${minutesLabel(stats.activity_window_seconds)} window.`
      : `No active tracks inside the rolling ${minutesLabel(
          stats.activity_window_seconds
        )} window. Holding registry positions — waiting for fresh edge detections.`;

  nodeSummaries.forEach((summary, index) => {
    const pos = resolvePosition(summary, index);
    positions.set(summary.nodeId, pos);
    const color = summary.alertCount > 0 ? "#ff716c" : ROLE_COLORS[summary.role] || ROLE_COLORS.fixed_tower;
    const marker = document.createElement("div");
    marker.className = "map-node";
    marker.style.left = `${pos.x}%`;
    marker.style.top = `${pos.y}%`;
    marker.innerHTML = `
      <div class="map-node-ring" style="border:1px dashed ${hexToRgba(color, 0.55)}"></div>
      <span class="material-symbols-outlined text-2xl" style="color:${color};font-variation-settings:'FILL' 1,'wght' 500,'GRAD' 0,'opsz' 24;">location_on</span>
      <span class="map-node-badge" style="border-color:${hexToRgba(color, 0.4)}">${shortenNodeLabel(summary.nodeId)}</span>`;
    markerLayer.appendChild(marker);
  });

  const pathNodes = nodeSummaries.filter((s) => s.active).length > 1
    ? nodeSummaries.filter((s) => s.active)
    : nodeSummaries.slice(0, 3);

  for (let i = 0; i < pathNodes.length - 1; i++) {
    const cur = positions.get(pathNodes[i].nodeId);
    const nxt = positions.get(pathNodes[i + 1].nodeId);
    if (!cur || !nxt) continue;
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", `${cur.x}%`); line.setAttribute("y1", `${cur.y}%`);
    line.setAttribute("x2", `${nxt.x}%`); line.setAttribute("y2", `${nxt.y}%`);
    line.setAttribute("stroke", "#50e1f9"); line.setAttribute("stroke-width", "2");
    line.setAttribute("stroke-dasharray", "10 5"); line.setAttribute("opacity", "0.85");
    linkLayer.appendChild(line);
  }

  activeRecords.slice(0, 18).forEach((record, index) => {
    const body = getBody(record);
    const nodePos = positions.get(body.node_id) || fallbackNodePosition({ nodeId: body.node_id }, index, activeRecords.length);
    const spread = 2 + (hashString(body.track_id) % 7);
    const dot = document.createElement("div");
    dot.className = "map-track-dot";
    dot.style.left = `${clamp(nodePos.x + (index % 3) * spread - spread, 6, 94)}%`;
    dot.style.top = `${clamp(nodePos.y + (index % 4) * spread - spread, 8, 92)}%`;
    dot.style.color = THREAT_COLORS[body.threat_level] || "#50e1f9";
    dot.style.background = THREAT_COLORS[body.threat_level] || "#50e1f9";
    markerLayer.appendChild(dot);
  });
}

function renderHeatmap(stats, activeRecords, nodeRegistry) {
  const grid = document.getElementById("heatmapGrid");
  grid.innerHTML = "";
  const cells = Array.from({ length: 64 }, () => ({ intensity: 0, color: "#20262f" }));
  const recordsToUse = activeRecords.length
    ? activeRecords
    : (nodeRegistry.nodes || []).map((n) => ({
        envelope: { body: { track_id: n.node_id, node_id: n.node_id, threat_level: "monitor" } },
      }));

  recordsToUse.forEach((record, i) => {
    const body = getBody(record);
    const ci = hashString(`${body.track_id}-${body.node_id}-${i}`) % 64;
    const col = body.threat_level === "high-interest" ? "#ffd16c" : body.node_id?.includes("relay") ? "#9df197" : "#50e1f9";
    cells[ci].intensity += 1; cells[ci].color = col;
    if (ci + 1 < 64) { cells[ci + 1].intensity += 0.35; cells[ci + 1].color = col; }
  });

  for (const cell of cells) {
    const el = document.createElement("div");
    el.className = "heat-cell";
    el.style.background = cell.intensity ? hexToRgba(cell.color, clamp(cell.intensity * 0.22, 0.08, 0.88)) : "#1b2028";
    grid.appendChild(el);
  }

  const sigma = 0.08 + stats.anomaly_probability * 0.8 + Math.min(stats.latest_track_count || 0, 20) * 0.003;
  document.getElementById("sigmaValue").textContent = `Sigma: ${sigma.toFixed(3)}`;
  document.getElementById("kernelValue").textContent = `Kernel: ${activeRecords.length ? "RBF-ST" : "PRIOR-ONLY"}`;
}

function renderConfidenceBars(stats, activeRecords) {
  const target = document.getElementById("confidenceBars");
  target.innerHTML = "";
  const activeCount = Math.max(activeRecords.length, 1);
  const benignCount = activeRecords.filter((r) => getBody(r).threat_level !== "high-interest").length;
  [
    { label: "Tactical Threat", value: stats.anomaly_probability, color: "#ff716c" },
    { label: "Civ/Friendly", value: benignCount / activeCount, color: "#9df197" },
    { label: "Infrastructure Load", value: stats.node_health_ratio, color: "#50e1f9" },
  ].forEach((row) => {
    const w = document.createElement("div");
    w.innerHTML = `
      <div class="confidence-row-label"><span>${row.label}</span><span>${formatPercent(row.value, 1)}</span></div>
      <div class="confidence-bar"><div class="confidence-fill" style="width:${clamp(row.value, 0, 1) * 100}%;background:${row.color};"></div></div>`;
    target.appendChild(w);
  });
}

function renderLearningFabric(stats, learningPlan, nodeRegistry, activeRecords) {
  const segments = document.getElementById("federatedSegments");
  segments.innerHTML = "";
  const filled = Math.round(clamp(stats.fed_alignment, 0, 1) * 10);
  for (let i = 0; i < 10; i++) {
    const seg = document.createElement("div");
    seg.className = "flex-1 h-full";
    seg.style.background = i < filled ? "#50e1f9" : "#20262f";
    segments.appendChild(seg);
  }

  const roundId = learningPlan.federated_round?.round_id;
  document.getElementById("federatedRoundLabel").textContent = `Round ${roundId ? String(roundId).slice(-4) : "--"} / ${stats.registered_node_count || "--"} nodes`;
  document.getElementById("federatedLossLabel").textContent = `Alignment: ${formatNumber(stats.fed_alignment, 2)}`;

  const activeNodeIds = new Set(activeRecords.map((r) => getBody(r).node_id));
  const rlGrid = document.getElementById("rlAgentGrid");
  rlGrid.innerHTML = "";
  (nodeRegistry.nodes || []).slice(0, 6).forEach((node) => {
    const active = activeNodeIds.has(node.node_id);
    const hasRl = (node.learning_layers || []).includes("rl");
    const cell = document.createElement("div");
    cell.className = "aspect-square flex items-center justify-center font-bold text-[0.65rem]";
    cell.style.background = hasRl ? "#d7383b" : active ? "#50e1f9" : "#5a606b";
    cell.style.color = hasRl ? "#190104" : active ? "#003239" : "#f1f3fc";
    cell.textContent = shortenNodeLabel(node.node_id).slice(0, 2);
    rlGrid.appendChild(cell);
  });

  const rlJobs = (learningPlan.reinforcement_learning || []).length;
  document.getElementById("criticStatus").textContent = rlJobs ? (activeNodeIds.size ? "ACTIVE" : "READY") : "IDLE";

  const supervised = (learningPlan.supervised_learning || []).length;
  const semiSupervised = (learningPlan.semi_supervised_learning || []).length;
  document.getElementById("learningNarrative").textContent = `The learning fabric is scheduling ${supervised} supervised jobs, ${semiSupervised} semi-supervised refreshes, and ${rlJobs} routing-policy updates. The dashboard treats these as readiness signals, not cumulative counters, so the panel stays meaningful even when the underlying journal keeps growing.`;
}

function modalityIcons(modalities) {
  const values = (modalities || []).map((m) => String(m).toLowerCase());
  if (!values.length) return "<span class='text-on-surface-variant'>--</span>";
  return values.map((m) => `<span class="material-symbols-outlined text-xs">${MODALITY_ICONS[m] || "sensors"}</span>`).join("");
}

function renderTrackLog(activeRecords, stats) {
  const target = document.getElementById("trackLogBody");
  target.innerHTML = "";
  document.getElementById("trackLogMeta").textContent = `Active detections, rolling ${minutesLabel(stats.activity_window_seconds)} window`;

  if (!activeRecords.length) {
    target.innerHTML = `<tr><td colspan="6" class="px-2 py-6 text-center"><div class="empty-state">No active tracks in rolling window</div></td></tr>`;
    return;
  }

  activeRecords.slice(0, 12).forEach((record, i) => {
    const body = getBody(record);
    const score = computeThreatScore(body);
    const row = document.createElement("tr");
    row.className = `border-b border-outline-variant/10 transition-colors hover:bg-surface-container-high ${i % 2 ? "bg-surface-container-low/20" : ""}`;
    row.innerHTML = `
      <td class="px-2 py-3 text-primary font-bold">#${body.track_id}</td>
      <td class="px-2 py-3">${body.node_id}</td>
      <td class="px-2 py-3"><span class="flex gap-1">${modalityIcons(body.contributing_modalities)}</span></td>
      <td class="px-2 py-3 ${body.threat_level === "high-interest" ? "text-error" : "text-tertiary"}">${score.toFixed(2)}</td>
      <td class="px-2 py-3 ${body.threat_level === "high-interest" ? "font-bold" : ""}">${Number(body.confidence || 0).toFixed(3)}</td>
      <td class="px-2 py-3 text-on-surface-variant">${formatTime(getRecordTimeMs(record))}</td>`;
    target.appendChild(row);
  });
}

function renderAlertsFeed(alerts, stats) {
  const target = document.getElementById("alertsFeed");
  target.innerHTML = "";
  document.getElementById("alertMeta").textContent = `Unique tracks in rolling ${minutesLabel(stats.activity_window_seconds)} window`;

  const cutoff = stats.active_cutoff_ms || 0;
  const recent = alerts.filter((r) => getRecordTimeMs(r) >= cutoff);
  if (!recent.length) {
    target.innerHTML = `<div class="empty-state">No high-interest alerts inside rolling window</div>`;
    return;
  }
  recent.slice(0, 8).forEach((record) => {
    const body = getBody(record);
    const card = document.createElement("article");
    card.className = "intel-card";
    card.innerHTML = `
      <header><strong>${body.track_id}</strong><span class="${badgeClassForThreat(body.threat_level)}">${formatLabel(body.threat_level)}</span></header>
      <p>${body.site || "Unknown site"} reporting ${formatLabel(body.threat_level)} posture at confidence ${Number(body.confidence || 0).toFixed(3)}.</p>
      <small>${body.node_id} | ${formatTime(getRecordTimeMs(record))} | ${(body.contributing_modalities || []).join(", ")}</small>`;
    target.appendChild(card);
  });
}

function renderOrchestrationFeed(orchestrationPlan) {
  const target = document.getElementById("orchestrationFeed");
  target.innerHTML = "";
  const digest = orchestrationPlan.policy_digest || {};
  const digestCard = document.createElement("article");
  digestCard.className = "intel-card";
  digestCard.innerHTML = `
    <header><strong>Protocol Digest</strong><span class="data-pill data-pill-secondary">${formatLabel(digest.high_priority_protocol || "none")}</span></header>
    <p>Priority traffic prefers ${formatLabel(digest.high_priority_protocol || "n/a")}, low-bandwidth exchange falls back to ${formatLabel(digest.low_bandwidth_protocol || "n/a")}, and regional exchange is handled over ${formatLabel(digest.regional_exchange_protocol || "n/a")}.</p>
    <small>Mesh discovery: ${formatLabel(digest.mesh_discovery_protocol || "n/a")}</small>`;
  target.appendChild(digestCard);

  const actions = [...(orchestrationPlan.routing_actions || []), ...(orchestrationPlan.relay_actions || [])];
  if (!actions.length) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = "No orchestration actions available";
    target.appendChild(empty);
    return;
  }
  actions.forEach((action) => {
    const card = document.createElement("article");
    card.className = "intel-card";
    const primaryLabel = action.priority || action.assignment || "planned";
    const secondary = action.preferred_protocol || action.target_zone || action.secondary_protocol || "n/a";
    card.innerHTML = `
      <header><strong>${action.node_id}</strong><span class="data-pill data-pill-primary">${formatLabel(primaryLabel)}</span></header>
      <p>${Object.entries(action).filter(([k]) => k !== "node_id").map(([k, v]) => `${formatLabel(k)}: ${formatLabel(v)}`).join(" | ")}</p>
      <small>Primary route target: ${formatLabel(secondary)}</small>`;
    target.appendChild(card);
  });
}

function renderGovernance(stats, governanceAudit) {
  const summary = document.getElementById("governanceSummary");
  const feed = document.getElementById("governanceFeed");
  summary.innerHTML = "";
  feed.innerHTML = "";

  [
    ["Registered Nodes", stats.registered_node_count || 0],
    ["Active Nodes", stats.active_node_count || 0],
    ["Trust Posture", stats.registered_node_count ? "Allowlisted" : "Open"],
    ["Recent Audit Events", governanceAudit.length],
  ].forEach(([label, value]) => {
    const row = document.createElement("div");
    row.className = "flex items-center justify-between border-b border-outline-variant/10 pb-2 font-mono text-[0.68rem] uppercase tracking-[0.08em]";
    row.innerHTML = `<span class="text-on-surface-variant">${label}</span><strong class="text-on-surface">${value}</strong>`;
    summary.appendChild(row);
  });

  if (!governanceAudit.length) {
    feed.innerHTML = `<div class="empty-state">No governance audit events yet</div>`;
    return;
  }
  governanceAudit.slice(0, 5).forEach((event) => {
    const card = document.createElement("article");
    card.className = "intel-card";
    card.innerHTML = `
      <header><strong>Round ${String(event.federated_round || "--").slice(-4)}</strong><span class="data-pill data-pill-tertiary">${formatLabel(event.regional_summary?.dominant_threat_level || "none")}</span></header>
      <p>Audit snapshot recorded ${event.regional_summary?.active_nodes || 0} active nodes and ${event.regional_summary?.active_tracks || 0} active tracks.</p>
      <small>${formatTime(event.timestamp_ms)} | Priority: ${formatLabel(event.policy_digest?.high_priority_protocol || "n/a")}</small>`;
    feed.appendChild(card);
  });
}

function renderNodeRegistry(nodeRegistry, stats, activeRecords) {
  const target = document.getElementById("nodeRegistryPanel");
  target.innerHTML = "";
  const activeNodeIds = new Set(activeRecords.map((r) => getBody(r).node_id));
  const nodes = nodeRegistry.nodes || [];
  if (!nodes.length) {
    target.innerHTML = `<div class="empty-state">No node registry data available</div>`;
    return;
  }
  nodes.forEach((node) => {
    const active = activeNodeIds.has(node.node_id);
    const card = document.createElement("article");
    card.className = "intel-card";
    card.innerHTML = `
      <header><strong>${node.node_id}</strong><span class="data-pill ${active ? "data-pill-secondary" : "data-pill-primary"}">${active ? "Active" : "Standby"}</span></header>
      <p>${formatLabel(node.role)} operating in ${formatLabel(node.zone)} with ${(node.capabilities || []).join(", ")} capability coverage.</p>
      <small>Protocols: ${(node.protocols || []).join(", ")} | Learning: ${(node.learning_layers || []).join(", ")}</small>`;
    target.appendChild(card);
  });
}

function buildTicker(stats, activeRecords, alerts, orchestrationPlan) {
  const parts = [];
  if (!connState.online) {
    parts.push(`⚠ Server unreachable — ${connState.consecutiveFailures} failure${connState.consecutiveFailures !== 1 ? "s" : ""}. Auto-retry in ${Math.round(connState.retryDelayMs / 1000)}s`);
  }
  if (activeRecords.length) {
    const body = getBody(activeRecords[0]);
    parts.push(`Node ${body.node_id} reported ${body.track_id} at confidence ${Number(body.confidence || 0).toFixed(3)}`);
    parts.push(`Dominant threat ${formatLabel(body.threat_level)}`);
    parts.push(`Active trackers ${stats.latest_track_count || 0}`);
    parts.push(`Throughput ${formatNumber(stats.throughput_events_per_min || 0, 1)} pkt/min`);
  } else if (alerts.length) {
    const body = getBody(alerts[0]);
    parts.push(`High-interest feed retained ${body.track_id} from ${body.node_id}`);
    parts.push(`Routing posture ${formatLabel(orchestrationPlan.policy_digest?.high_priority_protocol || "dds")}`);
    parts.push(`Waiting for new rolling-window activity`);
  } else {
    parts.push(`Rolling detection window ${minutesLabel(stats.activity_window_seconds)} active`);
    parts.push(`Registry posture intact`);
    parts.push(`Waiting for fresh semantic envelopes`);
  }
  if (connState.lastSuccessMs) parts.push(`Last sync ${formatRelativeTime(connState.lastSuccessMs)}`);
  return parts.join("  ·  ");
}

// ─── Last-known-good cache ────────────────────────────────────────────────────

let lastGoodData = null;

// ─── Core refresh ─────────────────────────────────────────────────────────────

async function refresh() {
  if (connState.isRefreshing) return;
  connState.isRefreshing = true;
  connState.lastAttemptMs = Date.now();
  updateConnectionBadge();

  // Lightweight health probe first
  const health = await loadJson("/healthz", null);

  if (health === null) {
    connState.consecutiveFailures++;
    connState.online = false;
    connState.isRefreshing = false;
    connState.retryDelayMs = Math.min(connState.retryDelayMs * 1.5, connState.maxRetryDelayMs);
    updateConnectionBadge();
    updateEndpointHealthPanel();
    renderOfflineBanner(true);

    document.getElementById("tickerStream").textContent = lastGoodData
      ? buildTicker(lastGoodData.stats, lastGoodData.activeRecords, lastGoodData.alerts, lastGoodData.orchestrationPlan)
      : `⚠ Server not responding on localhost:8090 — failure #${connState.consecutiveFailures}. Retry in ${Math.round(connState.retryDelayMs / 1000)}s.`;

    scheduleNextRefresh();
    return;
  }

  // Server alive — fetch all endpoints with individual per-endpoint fallbacks
  const EMPTY_STATS = {
    activity_window_seconds: 900, active_cutoff_ms: Date.now() - 900000,
    latest_track_count: 0, high_interest_recent_count: 0, active_high_interest_count: 0,
    node_counts: {}, threat_counts: {}, modality_counts: {}, site_counts: {},
    registered_node_count: 0, active_node_count: 0, throughput_events_per_min: 0,
    anomaly_probability: 0, node_health_ratio: 0, fed_alignment: 0,
    federated_participant_count: 0, recent_journal_count: 0, last_detection_ms: null, stale: true,
  };

  const [stats, latest, alerts, regionalSummary, learningPlan, orchestrationPlan, nodeRegistry, governanceAudit] =
    await Promise.all([
      loadJson("/api/stats", EMPTY_STATS),
      loadJson("/api/latest", {}),
      loadJson("/api/high-interest?limit=40", []),
      loadJson("/api/regional-summary", { region: "Bwari, FCT" }),
      loadJson("/api/learning-plan", { supervised_learning: [], reinforcement_learning: [], semi_supervised_learning: [], federated_round: {} }),
      loadJson("/api/orchestration", { policy_digest: {}, routing_actions: [], relay_actions: [] }),
      loadJson("/api/node-registry", { nodes: [], cluster_id: "uriel orchestrator" }),
      loadJson("/api/governance-audit?limit=10", []),
    ]);

  connState.online = true;
  connState.consecutiveFailures = 0;
  connState.lastSuccessMs = Date.now();
  connState.retryDelayMs = 2000;
  connState.isRefreshing = false;

  renderOfflineBanner(false);
  updateConnectionBadge();
  updateEndpointHealthPanel();
  renderDataStalenessWarning(stats);

  const activeRecords = getActiveRecords(latest, stats.active_cutoff_ms || 0);
  lastGoodData = { stats, activeRecords, alerts, orchestrationPlan };

  updateHeader(stats, regionalSummary, nodeRegistry);
  renderMetricCards(stats);
  setMissionMode(determineMissionMode(stats, regionalSummary));
  document.getElementById("missionNarrative").textContent = buildMissionNarrative(stats, regionalSummary, learningPlan, nodeRegistry);
  renderMap(stats, regionalSummary, activeRecords, nodeRegistry);
  renderHeatmap(stats, activeRecords, nodeRegistry);
  renderConfidenceBars(stats, activeRecords);
  renderLearningFabric(stats, learningPlan, nodeRegistry, activeRecords);
  renderTrackLog(activeRecords, stats);
  renderAlertsFeed(alerts, stats);
  renderOrchestrationFeed(orchestrationPlan);
  renderGovernance(stats, governanceAudit);
  renderNodeRegistry(nodeRegistry, stats, activeRecords);
  document.getElementById("tickerStream").textContent = buildTicker(stats, activeRecords, alerts, orchestrationPlan);

  scheduleNextRefresh();
}

// ─── Scheduler with countdown ─────────────────────────────────────────────────

function scheduleNextRefresh() {
  clearTimeout(connState.refreshTimer);
  const delay = connState.online ? connState.refreshIntervalMs : connState.retryDelayMs;
  connState.refreshTimer = setTimeout(() => refresh().catch(console.error), delay);

  // Live countdown in badge when offline
  if (!connState.online) {
    let remaining = Math.round(delay / 1000);
    const ticker = setInterval(() => {
      remaining--;
      const label = document.getElementById("connLabel");
      if (!label || connState.online || remaining <= 0) { clearInterval(ticker); return; }
      label.textContent = `Offline · retry in ${remaining}s`;
    }, 1000);
  }
}

// ─── Boot ─────────────────────────────────────────────────────────────────────

document.getElementById("refreshFeed").addEventListener("click", () => {
  clearTimeout(connState.refreshTimer);
  connState.retryDelayMs = 2000;
  refresh().catch(console.error);
});

document.getElementById("retryBtn")?.addEventListener("click", () => {
  clearTimeout(connState.refreshTimer);
  connState.retryDelayMs = 2000;
  refresh().catch(console.error);
});

updateConnectionBadge();
refresh().catch((err) => {
  console.error(err);
  document.getElementById("tickerStream").textContent = `Console error: ${err.message}`;
});