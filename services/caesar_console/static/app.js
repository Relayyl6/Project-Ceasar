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

async function loadJson(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Request failed: ${path}`);
  }
  return response.json();
}

function getBody(record) {
  return record?.envelope?.body ?? {};
}

function getRecordTimeMs(record) {
  return record?.received_at_ms ?? record?.timestamp_ms ?? getBody(record)?.timestamp_ms ?? 0;
}

function formatLabel(label) {
  return String(label ?? "")
    .replaceAll("_", " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
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
    .filter((record) => getRecordTimeMs(record) >= cutoffMs && getBody(record).track_id)
    .sort((a, b) => getRecordTimeMs(b) - getRecordTimeMs(a));
}

function computeThreatScore(body) {
  const baseConfidence = Number(body.confidence || 0);
  if (body.threat_level === "high-interest") {
    return clamp(baseConfidence, 0, 0.999);
  }
  if (body.threat_level === "monitor") {
    return clamp(baseConfidence * 0.42, 0, 0.999);
  }
  return clamp(baseConfidence * 0.18, 0, 0.999);
}

function determineMissionMode(stats, regionalSummary) {
  if ((stats.active_high_interest_count || 0) > 0 || regionalSummary.dominant_threat_level === "high-interest") {
    return "tactical";
  }
  const siteText = JSON.stringify(regionalSummary.site_activity || {}).toLowerCase();
  if (siteText.includes("farm") || siteText.includes("field") || siteText.includes("irrigation")) {
    return "agriculture";
  }
  if ((stats.active_node_count || 0) > 0) {
    return "surveillance";
  }
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
    return `The command layer is monitoring ${registeredNodes} registered nodes across ${regionalSummary.region || "the region"}. No active tracks are inside the rolling ${minutesLabel(
      stats.activity_window_seconds,
    )} window, so the console is emphasizing registry posture, orchestration policy, and learning readiness instead of stale totals.`;
  }

  return `The mesh is coordinating ${activeTracks} active tracks across ${activeNodes} live nodes in ${
    regionalSummary.region || "the region"
  }. ${highInterest} high-interest track${highInterest === 1 ? "" : "s"} currently shape routing priority, while ${supervisedJobs} supervised and ${rlJobs} reinforcement jobs keep the learning fabric aligned with agriculture, infrastructure, surveillance, and tactical mission demand.`;
}

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
  const registryMap = new Map((nodeRegistry.nodes || []).map((node) => [node.node_id, node]));
  const groups = new Map();

  for (const record of activeRecords) {
    const body = getBody(record);
    if (!groups.has(body.node_id)) {
      groups.set(body.node_id, []);
    }
    groups.get(body.node_id).push(body);
  }

  const summaries = [];
  for (const [nodeId, tracks] of groups.entries()) {
    const registry = registryMap.get(nodeId) || {};
    const latitudes = tracks.map((track) => Number(track.geo_latitude)).filter(Number.isFinite);
    const longitudes = tracks.map((track) => Number(track.geo_longitude)).filter(Number.isFinite);
    summaries.push({
      nodeId,
      role: registry.role || "fixed_tower",
      zone: registry.zone || tracks[0]?.site || "unknown",
      protocols: registry.protocols || [],
      learningLayers: registry.learning_layers || [],
      capabilities: registry.capabilities || [],
      tracks,
      alertCount: tracks.filter((track) => track.threat_level === "high-interest").length,
      latitude: latitudes.length ? latitudes.reduce((sum, value) => sum + value, 0) / latitudes.length : null,
      longitude: longitudes.length ? longitudes.reduce((sum, value) => sum + value, 0) / longitudes.length : null,
      active: true,
    });
  }

  if (!summaries.length) {
    return (nodeRegistry.nodes || []).map((node, index) => ({
      nodeId: node.node_id,
      role: node.role,
      zone: node.zone,
      protocols: node.protocols || [],
      learningLayers: node.learning_layers || [],
      capabilities: node.capabilities || [],
      tracks: [],
      alertCount: 0,
      latitude: null,
      longitude: null,
      active: false,
      fallbackIndex: index,
    }));
  }

  for (const node of nodeRegistry.nodes || []) {
    if (!groups.has(node.node_id)) {
      summaries.push({
        nodeId: node.node_id,
        role: node.role,
        zone: node.zone,
        protocols: node.protocols || [],
        learningLayers: node.learning_layers || [],
        capabilities: node.capabilities || [],
        tracks: [],
        alertCount: 0,
        latitude: null,
        longitude: null,
        active: false,
      });
    }
  }

  return summaries;
}

function fallbackNodePosition(summary, index, total) {
  const hash = hashString(summary.nodeId);
  if (summary.role === "regional_hub") {
    return { x: 74, y: 68 };
  }
  if (summary.role === "relay") {
    return { x: 52 + (hash % 16), y: 26 + (hash % 10) };
  }
  const columns = Math.max(2, Math.ceil(Math.sqrt(total || 1)));
  const row = Math.floor(index / columns);
  const col = index % columns;
  return {
    x: 20 + col * (52 / Math.max(columns - 1, 1)) + (hash % 7),
    y: 28 + row * 18 + (hash % 6),
  };
}

function buildPositionResolver(nodeSummaries) {
  const geoNodes = nodeSummaries.filter(
    (summary) => Number.isFinite(summary.latitude) && Number.isFinite(summary.longitude),
  );

  if (geoNodes.length >= 2) {
    const latitudes = geoNodes.map((node) => node.latitude);
    const longitudes = geoNodes.map((node) => node.longitude);
    const minLat = Math.min(...latitudes);
    const maxLat = Math.max(...latitudes);
    const minLon = Math.min(...longitudes);
    const maxLon = Math.max(...longitudes);
    const latSpan = maxLat - minLat;
    const lonSpan = maxLon - minLon;

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
      ? `${stats.latest_track_count} live tracks are being reconciled across ${stats.active_node_count} active nodes. Dominant threat posture is ${formatLabel(
          regionalSummary.dominant_threat_level || "none",
        )}, and the map is only showing detections inside the rolling ${minutesLabel(stats.activity_window_seconds)} window.`
      : `No active tracks are inside the rolling ${minutesLabel(
          stats.activity_window_seconds,
        )} window. The map is holding registry positions and waiting for fresh edge detections rather than displaying stale historical tracks.`;

  nodeSummaries.forEach((summary, index) => {
    const position = resolvePosition(summary, index);
    positions.set(summary.nodeId, position);
    const color =
      summary.alertCount > 0 ? "#ff716c" : ROLE_COLORS[summary.role] || ROLE_COLORS.fixed_tower;

    const marker = document.createElement("div");
    marker.className = "map-node";
    marker.style.left = `${position.x}%`;
    marker.style.top = `${position.y}%`;
    marker.innerHTML = `
      <div class="map-node-ring" style="border:1px dashed ${hexToRgba(color, 0.55)}"></div>
      <span class="material-symbols-outlined text-2xl" style="color:${color};font-variation-settings:'FILL' 1,'wght' 500,'GRAD' 0,'opsz' 24;">location_on</span>
      <span class="map-node-badge" style="border-color:${hexToRgba(color, 0.4)}">${shortenNodeLabel(summary.nodeId)}</span>
    `;
    markerLayer.appendChild(marker);
  });

  const connectedNodes = nodeSummaries.filter((summary) => summary.active);
  const pathNodes = connectedNodes.length > 1 ? connectedNodes : nodeSummaries.slice(0, 3);
  for (let index = 0; index < pathNodes.length - 1; index += 1) {
    const current = positions.get(pathNodes[index].nodeId);
    const next = positions.get(pathNodes[index + 1].nodeId);
    if (!current || !next) continue;
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.setAttribute("x1", `${current.x}%`);
    line.setAttribute("y1", `${current.y}%`);
    line.setAttribute("x2", `${next.x}%`);
    line.setAttribute("y2", `${next.y}%`);
    line.setAttribute("stroke", "#50e1f9");
    line.setAttribute("stroke-width", "2");
    line.setAttribute("stroke-dasharray", "10 5");
    line.setAttribute("opacity", "0.85");
    linkLayer.appendChild(line);
  }

  activeRecords.slice(0, 18).forEach((record, index) => {
    const body = getBody(record);
    const nodePosition = positions.get(body.node_id) || fallbackNodePosition({ nodeId: body.node_id }, index, activeRecords.length);
    const spread = 2 + (hashString(body.track_id) % 7);
    const dot = document.createElement("div");
    dot.className = "map-track-dot";
    dot.style.left = `${clamp(nodePosition.x + (index % 3) * spread - spread, 6, 94)}%`;
    dot.style.top = `${clamp(nodePosition.y + (index % 4) * spread - spread, 8, 92)}%`;
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
    : (nodeRegistry.nodes || []).map((node) => ({ envelope: { body: { track_id: node.node_id, node_id: node.node_id, threat_level: "monitor" } } }));

  recordsToUse.forEach((record, index) => {
    const body = getBody(record);
    const cellIndex = hashString(`${body.track_id}-${body.node_id}-${index}`) % 64;
    const threatColor =
      body.threat_level === "high-interest"
        ? "#ffd16c"
        : body.node_id?.includes("relay")
          ? "#9df197"
          : "#50e1f9";
    cells[cellIndex].intensity += 1;
    cells[cellIndex].color = threatColor;
    if (cellIndex + 1 < 64) {
      cells[cellIndex + 1].intensity += 0.35;
      cells[cellIndex + 1].color = threatColor;
    }
  });

  for (const cell of cells) {
    const element = document.createElement("div");
    element.className = "heat-cell";
    const alpha = clamp(cell.intensity * 0.22, 0.08, 0.88);
    element.style.background = cell.intensity ? hexToRgba(cell.color, alpha) : "#1b2028";
    grid.appendChild(element);
  }

  const sigma = 0.08 + stats.anomaly_probability * 0.8 + Math.min(stats.latest_track_count || 0, 20) * 0.003;
  document.getElementById("sigmaValue").textContent = `Sigma: ${sigma.toFixed(3)}`;
  document.getElementById("kernelValue").textContent = `Kernel: ${activeRecords.length ? "RBF-ST" : "PRIOR-ONLY"}`;
}

function renderConfidenceBars(stats, activeRecords) {
  const target = document.getElementById("confidenceBars");
  target.innerHTML = "";
  const activeCount = Math.max(activeRecords.length, 1);
  const benignCount = activeRecords.filter((record) => getBody(record).threat_level !== "high-interest").length;
  const rows = [
    { label: "Tactical Threat", value: stats.anomaly_probability, color: "#ff716c" },
    { label: "Civ/Friendly", value: benignCount / activeCount, color: "#9df197" },
    { label: "Infrastructure Load", value: stats.node_health_ratio, color: "#50e1f9" },
  ];

  rows.forEach((row) => {
    const wrapper = document.createElement("div");
    wrapper.innerHTML = `
      <div class="confidence-row-label">
        <span>${row.label}</span>
        <span>${formatPercent(row.value, 1)}</span>
      </div>
      <div class="confidence-bar">
        <div class="confidence-fill" style="width:${clamp(row.value, 0, 1) * 100}%;background:${row.color};"></div>
      </div>
    `;
    target.appendChild(wrapper);
  });
}

function renderLearningFabric(stats, learningPlan, nodeRegistry, activeRecords) {
  const segments = document.getElementById("federatedSegments");
  segments.innerHTML = "";
  const totalSegments = 10;
  const filledSegments = Math.round(clamp(stats.fed_alignment, 0, 1) * totalSegments);

  for (let index = 0; index < totalSegments; index += 1) {
    const segment = document.createElement("div");
    segment.className = "flex-1 h-full";
    segment.style.background = index < filledSegments ? "#50e1f9" : "#20262f";
    segments.appendChild(segment);
  }

  const roundId = learningPlan.federated_round?.round_id;
  document.getElementById("federatedRoundLabel").textContent = `Round ${roundId ? String(roundId).slice(-4) : "--"} / ${
    stats.registered_node_count || "--"
  } nodes`;
  document.getElementById("federatedLossLabel").textContent = `Alignment: ${formatNumber(stats.fed_alignment, 2)}`;

  const activeNodeIds = new Set(activeRecords.map((record) => getBody(record).node_id));
  const rlGrid = document.getElementById("rlAgentGrid");
  rlGrid.innerHTML = "";
  (nodeRegistry.nodes || []).slice(0, 6).forEach((node) => {
    const cell = document.createElement("div");
    const active = activeNodeIds.has(node.node_id);
    const hasRl = (node.learning_layers || []).includes("rl");
    const background = hasRl ? "#d7383b" : active ? "#50e1f9" : "#5a606b";
    const foreground = hasRl ? "#190104" : active ? "#003239" : "#f1f3fc";
    cell.className = "aspect-square flex items-center justify-center font-bold text-[0.65rem]";
    cell.style.background = background;
    cell.style.color = foreground;
    cell.textContent = shortenNodeLabel(node.node_id).slice(0, 2);
    rlGrid.appendChild(cell);
  });

  const reinforcementJobs = (learningPlan.reinforcement_learning || []).length;
  const criticStatus = reinforcementJobs
    ? activeNodeIds.size
      ? "ACTIVE"
      : "READY"
    : "IDLE";
  document.getElementById("criticStatus").textContent = criticStatus;

  const supervised = (learningPlan.supervised_learning || []).length;
  const semiSupervised = (learningPlan.semi_supervised_learning || []).length;
  document.getElementById("learningNarrative").textContent = `The learning fabric is scheduling ${supervised} supervised jobs, ${semiSupervised} semi-supervised refreshes, and ${reinforcementJobs} routing-policy updates. The dashboard treats these as readiness signals, not cumulative counters, so the panel stays meaningful even when the underlying journal keeps growing.`;
}

function modalityIcons(modalities) {
  const values = (modalities || []).map((modality) => String(modality).toLowerCase());
  if (!values.length) return "<span class='text-on-surface-variant'>--</span>";
  return values
    .map((modality) => {
      const icon = MODALITY_ICONS[modality] || "sensors";
      return `<span class="material-symbols-outlined text-xs">${icon}</span>`;
    })
    .join("");
}

function renderTrackLog(activeRecords, stats) {
  const target = document.getElementById("trackLogBody");
  target.innerHTML = "";
  document.getElementById("trackLogMeta").textContent = `Active detections, rolling ${minutesLabel(stats.activity_window_seconds)} window`;

  if (!activeRecords.length) {
    target.innerHTML = `<tr><td colspan="6" class="px-2 py-6 text-center"><div class="empty-state">No active tracks in rolling window</div></td></tr>`;
    return;
  }

  activeRecords.slice(0, 12).forEach((record, index) => {
    const body = getBody(record);
    const threatScore = computeThreatScore(body);
    const row = document.createElement("tr");
    row.className = `border-b border-outline-variant/10 transition-colors hover:bg-surface-container-high ${
      index % 2 ? "bg-surface-container-low/20" : ""
    }`;
    row.innerHTML = `
      <td class="px-2 py-3 text-primary font-bold">#${body.track_id}</td>
      <td class="px-2 py-3">${body.node_id}</td>
      <td class="px-2 py-3"><span class="flex gap-1">${modalityIcons(body.contributing_modalities)}</span></td>
      <td class="px-2 py-3 ${body.threat_level === "high-interest" ? "text-error" : "text-tertiary"}">${threatScore.toFixed(2)}</td>
      <td class="px-2 py-3 ${body.threat_level === "high-interest" ? "font-bold" : ""}">${Number(body.confidence || 0).toFixed(3)}</td>
      <td class="px-2 py-3 text-on-surface-variant">${formatTime(getRecordTimeMs(record))}</td>
    `;
    target.appendChild(row);
  });
}

function renderAlertsFeed(alerts, stats) {
  const target = document.getElementById("alertsFeed");
  target.innerHTML = "";
  document.getElementById("alertMeta").textContent = `Unique tracks in rolling ${minutesLabel(stats.activity_window_seconds)} window`;

  const cutoff = stats.active_cutoff_ms || 0;
  const recentAlerts = alerts.filter((record) => getRecordTimeMs(record) >= cutoff);
  if (!recentAlerts.length) {
    target.innerHTML = `<div class="empty-state">No high-interest alerts inside rolling window</div>`;
    return;
  }

  recentAlerts.slice(0, 8).forEach((record) => {
    const body = getBody(record);
    const card = document.createElement("article");
    card.className = "intel-card";
    card.innerHTML = `
      <header>
        <strong>${body.track_id}</strong>
        <span class="${badgeClassForThreat(body.threat_level)}">${formatLabel(body.threat_level)}</span>
      </header>
      <p>${body.site || "Unknown site"} reporting ${formatLabel(body.threat_level)} posture at confidence ${Number(
        body.confidence || 0,
      ).toFixed(3)}.</p>
      <small>${body.node_id} | ${formatTime(getRecordTimeMs(record))} | ${(body.contributing_modalities || []).join(", ")}</small>
    `;
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
    <header>
      <strong>Protocol Digest</strong>
      <span class="data-pill data-pill-secondary">${formatLabel(digest.high_priority_protocol || "none")}</span>
    </header>
    <p>Priority traffic prefers ${formatLabel(digest.high_priority_protocol || "n/a")}, low-bandwidth exchange falls back to ${formatLabel(
      digest.low_bandwidth_protocol || "n/a",
    )}, and regional exchange is handled over ${formatLabel(digest.regional_exchange_protocol || "n/a")}.</p>
    <small>Mesh discovery: ${formatLabel(digest.mesh_discovery_protocol || "n/a")}</small>
  `;
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
      <header>
        <strong>${action.node_id}</strong>
        <span class="data-pill data-pill-primary">${formatLabel(primaryLabel)}</span>
      </header>
      <p>${Object.entries(action)
        .filter(([key]) => key !== "node_id")
        .map(([key, value]) => `${formatLabel(key)}: ${formatLabel(value)}`)
        .join(" | ")}</p>
      <small>Primary route target: ${formatLabel(secondary)}</small>
    `;
    target.appendChild(card);
  });
}

function renderGovernance(stats, governanceAudit) {
  const summary = document.getElementById("governanceSummary");
  const feed = document.getElementById("governanceFeed");
  summary.innerHTML = "";
  feed.innerHTML = "";

  const rows = [
    ["Registered Nodes", stats.registered_node_count || 0],
    ["Active Nodes", stats.active_node_count || 0],
    ["Trust Posture", stats.registered_node_count ? "Allowlisted" : "Open"],
    ["Recent Audit Events", governanceAudit.length],
  ];

  rows.forEach(([label, value]) => {
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
      <header>
        <strong>Round ${String(event.federated_round || "--").slice(-4)}</strong>
        <span class="data-pill data-pill-tertiary">${formatLabel(
          event.regional_summary?.dominant_threat_level || "none",
        )}</span>
      </header>
      <p>Audit snapshot recorded ${event.regional_summary?.active_nodes || 0} active nodes and ${
        event.regional_summary?.active_tracks || 0
      } active tracks.</p>
      <small>${formatTime(event.timestamp_ms)} | Priority: ${formatLabel(event.policy_digest?.high_priority_protocol || "n/a")}</small>
    `;
    feed.appendChild(card);
  });
}

function renderNodeRegistry(nodeRegistry, stats, activeRecords) {
  const target = document.getElementById("nodeRegistryPanel");
  target.innerHTML = "";
  const activeNodeIds = new Set(activeRecords.map((record) => getBody(record).node_id));
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
      <header>
        <strong>${node.node_id}</strong>
        <span class="data-pill ${active ? "data-pill-secondary" : "data-pill-primary"}">${active ? "Active" : "Standby"}</span>
      </header>
      <p>${formatLabel(node.role)} operating in ${formatLabel(node.zone)} with ${(
        node.capabilities || []
      ).join(", ")} capability coverage.</p>
      <small>Protocols: ${(node.protocols || []).join(", ")} | Learning: ${(node.learning_layers || []).join(", ")}</small>
    `;
    target.appendChild(card);
  });
}

function buildTicker(stats, activeRecords, alerts, orchestrationPlan) {
  if (activeRecords.length) {
    const record = activeRecords[0];
    const body = getBody(record);
    return `Node ${body.node_id} reported ${body.track_id} at confidence ${Number(body.confidence || 0).toFixed(
      3,
    )}... Dominant threat ${formatLabel(body.threat_level)}... Active trackers ${stats.latest_track_count || 0}... Rolling latency proxy ${
      formatNumber(stats.throughput_events_per_min || 0, 1)
    } pkt/min...`;
  }
  if (alerts.length) {
    const body = getBody(alerts[0]);
    return `High-interest feed retained ${body.track_id} from ${body.node_id}... Routing posture ${formatLabel(
      orchestrationPlan.policy_digest?.high_priority_protocol || "dds",
    )}... Waiting for new rolling-window activity...`;
  }
  return `Rolling detection window ${minutesLabel(
    stats.activity_window_seconds,
  )} active... Registry posture intact... Waiting for fresh semantic envelopes...`;
}

async function refresh() {
  const [stats, latest, alerts, regionalSummary, learningPlan, orchestrationPlan, nodeRegistry, governanceAudit] =
    await Promise.all([
      loadJson("/api/stats"),
      loadJson("/api/latest"),
      loadJson("/api/high-interest?limit=40"),
      loadJson("/api/regional-summary"),
      loadJson("/api/learning-plan"),
      loadJson("/api/orchestration"),
      loadJson("/api/node-registry"),
      loadJson("/api/governance-audit?limit=10"),
    ]);

  const activeRecords = getActiveRecords(latest, stats.active_cutoff_ms || 0);
  updateHeader(stats, regionalSummary, nodeRegistry);
  renderMetricCards(stats);

  const missionMode = determineMissionMode(stats, regionalSummary);
  setMissionMode(missionMode);
  document.getElementById("missionNarrative").textContent = buildMissionNarrative(
    stats,
    regionalSummary,
    learningPlan,
    nodeRegistry,
  );

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
}

document.getElementById("refreshFeed").addEventListener("click", refresh);
refresh().catch((error) => {
  console.error(error);
  document.getElementById("tickerStream").textContent = `Console refresh error: ${error.message}`;
});
setInterval(() => {
  refresh().catch((error) => {
    console.error(error);
    document.getElementById("tickerStream").textContent = `Console refresh error: ${error.message}`;
  });
}, 8000);
