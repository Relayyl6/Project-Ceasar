/**
 * caesar-api.js  —  Shared data layer for all three Project Caesar dashboards
 *
 * Polls the caesar console server at localhost:8090 every POLL_MS.
 * On every successful fetch, fires CustomEvents on window:
 *   "caesar:stats"            → detail: stats object
 *   "caesar:latest"           → detail: latest tracks dict
 *   "caesar:high-interest"    → detail: list of alert records
 *   "caesar:regional-summary" → detail: regional summary object
 *   "caesar:learning-plan"    → detail: learning plan object
 *   "caesar:orchestration"    → detail: orchestration plan object
 *   "caesar:node-registry"    → detail: node registry object
 *   "caesar:governance-audit" → detail: list of audit events
 *   "caesar:tick"             → detail: { online, lastSyncMs, stale, state }
 *   "caesar:online"           → fires when connection is established
 *   "caesar:offline"          → fires when connection is lost
 *
 * When offline, all events continue firing with the last known good data
 * merged with lightweight simulation so dashboards stay animated.
 */

(function (global) {
  "use strict";

  // ── CONFIG ─────────────────────────────────────────────────────────────────
  const BASE =
    global.location && /^https?:/i.test(global.location.origin || "")
      ? global.location.origin
      : "http://localhost:8090";
  const POLL_MS   = 8000;        // background poll (SSE handles real-time)
  const HEALTH_MS = 8000;        // health-check interval when offline
  const SIM_MS    = 1200;        // simulation tick between polls

  // ── STATE ──────────────────────────────────────────────────────────────────
  const state = {
    online: false,
    consecutiveFailures: 0,
    lastSuccessMs: null,
    lastAttemptMs: null,

    // Last-known-good data from server
    stats:           null,
    latest:          null,
    highInterest:    null,
    regionalSummary: null,
    learningPlan:    null,
    orchestration:   null,
    nodeRegistry:    null,
    governanceAudit: null,

    // Simulation state (used when offline or to augment live data)
    sim: {
      trackCount:   80,
      throughput:   1.1,
      anomalyProb:  0.04,
      alignment:    0.88,
      fedRound:     100,
      fedLoss:      0.008,
      soilMoisture: 62.0,
      ndvi:         0.80,
      humidity:     82.0,
      pressure:     14.2,
      vibration:    0.018,
      gridConn:     99.1,
      astEff:       90,
      waterSavings: 11800,
      vtolBat:      85,
      swarmUnits:   128,
    },
  };

  // ── HELPERS ────────────────────────────────────────────────────────────────
  function emit(name, detail) {
    global.dispatchEvent(new CustomEvent(name, { detail }));
  }

  function drift(v, speed, lo, hi) {
    return Math.max(lo, Math.min(hi, v + (Math.random() - 0.495) * speed));
  }

  async function fetchJSON(path, fallback = null) {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), 5000);
    try {
      const r = await fetch(BASE + path, { cache: "no-store", signal: ctrl.signal });
      clearTimeout(timer);
      if (!r.ok) return fallback;
      return await r.json();
    } catch {
      clearTimeout(timer);
      return fallback;
    }
  }

  // ── SIMULATION: build synthetic objects matching real API shapes ───────────
  function simStats() {
    const s = state.sim;
    return {
      activity_window_seconds: 900,
      active_cutoff_ms: Date.now() - 900000,
      latest_track_count: Math.round(s.trackCount),
      high_interest_recent_count: Math.round(s.trackCount * 0.08),
      active_high_interest_count: Math.round(s.trackCount * 0.05),
      node_counts: { "tower-alpha": 28, "tower-bravo": 22, "orch-beta": 18, "relay-01": 12, "relay-02": 10 },
      threat_counts: {
        "high-interest": Math.round(s.trackCount * 0.05),
        monitor: Math.round(s.trackCount * 0.28),
        none: Math.round(s.trackCount * 0.67),
      },
      modality_counts: { optical: 48, thermal: 31, radar: 27, manual: 14 },
      site_counts: { "bwari-north": 34, "bwari-east": 28, "bwari-central": 18 },
      registered_node_count: 6,
      active_node_count: 5,
      throughput_events_per_min: +s.throughput.toFixed(2),
      anomaly_probability: +s.anomalyProb.toFixed(4),
      node_health_ratio: 0.833,
      fed_alignment: +s.alignment.toFixed(4),
      federated_participant_count: 5,
      recent_journal_count: Math.round(s.trackCount * 1.4),
      last_detection_ms: Date.now() - Math.round(Math.random() * 60000),
      stale: false,
      // ── agri extras ──
      soil_moisture_pct: +s.soilMoisture.toFixed(1),
      ndvi_index: +s.ndvi.toFixed(3),
      humidity_pct: +s.humidity.toFixed(1),
      max_yield_t_ha: +(7.5 + s.soilMoisture * 0.02 + s.ndvi * 1.2).toFixed(2),
      ast_efficiency_pct: +s.astEff.toFixed(0),
      water_savings_l_wk: Math.round(s.waterSavings),
      swarm_units: Math.round(s.swarmUnits),
      vtol_battery_pct: +s.vtolBat.toFixed(0),
      system_integrity_pct: +(98.5 + s.alignment * 1.3).toFixed(2),
      // ── infra extras ──
      grid_connectivity_pct: +s.gridConn.toFixed(1),
      pipeline_pressure_bar: +s.pressure.toFixed(1),
      vibration_rms: +s.vibration.toFixed(3),
    };
  }

  function simLatest() {
    const nodes = ["tower-alpha", "tower-bravo", "tower-gamma", "orch-beta", "relay-01", "relay-02"];
    const threats = ["none", "none", "none", "monitor", "monitor", "high-interest"];
    const modalities = [["optical"], ["thermal"], ["radar"], ["optical", "thermal"], ["radar", "manual"]];
    const out = {};
    for (let i = 0; i < 12; i++) {
      const id = `SIM-TRK-${9900 + i}`;
      const node = nodes[i % nodes.length];
      out[id] = {
        received_at_ms: Date.now() - Math.round(Math.random() * 600000),
        envelope: {
          body: {
            track_id: id,
            node_id: node,
            site: node.includes("alpha") ? "bwari-north" : node.includes("bravo") ? "bwari-east" : "bwari-central",
            threat_level: threats[i % threats.length],
            confidence: +(0.72 + Math.random() * 0.27).toFixed(3),
            contributing_modalities: modalities[i % modalities.length],
            geo_latitude: 9.28 + (Math.random() - 0.5) * 0.12,
            geo_longitude: 7.38 + (Math.random() - 0.5) * 0.12,
            timestamp_ms: Date.now() - Math.round(Math.random() * 600000),
          },
        },
      };
    }
    return out;
  }

  function simHighInterest() {
    return Object.values(simLatest())
      .filter(r => r.envelope.body.threat_level === "high-interest")
      .slice(0, 5);
  }

  function simRegionalSummary() {
    return {
      cluster_id: "uriel-bwari-alpha",
      region: "Bwari, FCT",
      active_node_count: 5,
      active_track_count: Math.round(state.sim.trackCount),
      high_interest_recent_count: Math.round(state.sim.trackCount * 0.08),
      dominant_threat_level: state.sim.anomalyProb > 0.1 ? "high-interest" : "monitor",
      threat_counts: { "high-interest": 5, monitor: 32, none: 67 },
      modality_counts: { optical: 48, thermal: 31, radar: 27, manual: 14 },
      site_activity: { "bwari-north": 34, "bwari-east": 28, "bwari-central": 18 },
    };
  }

  function simLearningPlan() {
    return {
      cluster_id: "uriel-bwari-alpha",
      supervised_learning: [
        { node_id: "tower-alpha", job_type: "supervised_recalibration", target_model: "detector-head", label_budget: 40, trigger: "regional-threat-drift" },
        { node_id: "tower-bravo", job_type: "supervised_recalibration", target_model: "detector-head", label_budget: 30, trigger: "regional-threat-drift" },
      ],
      semi_supervised_learning: [
        { node_id: "relay-01", job_type: "anomaly_autoencoder_refresh", target_model: "environmental-anomaly-detector", window_size: 100, trigger: "confidence-spread-shift" },
      ],
      reinforcement_learning: [
        { node_id: "orch-beta", job_type: "routing_policy_update", target_policy: "mesh-traffic-coordinator", reward_signal: "alert_delivery_latency_vs_bandwidth", trigger: "relay-load-change" },
      ],
      federated_round: {
        round_id: state.sim.fedRound,
        strategy: "fedavg",
        participants: ["tower-alpha", "tower-bravo", "tower-gamma", "orch-beta", "relay-01"],
        aggregation_target: "orch-beta",
        global_models: ["detector-head", "environmental-anomaly-detector", "mesh-traffic-coordinator"],
      },
    };
  }

  function simOrchestration() {
    return {
      cluster_id: "uriel-bwari-alpha",
      policy_digest: {
        high_priority_protocol: "dds",
        low_bandwidth_protocol: "mqtt",
        mesh_discovery_protocol: "zenoh",
        regional_exchange_protocol: "amqp",
      },
      routing_actions: [
        { node_id: "tower-alpha", priority: "high", preferred_protocol: "dds", secondary_protocol: "zenoh" },
        { node_id: "tower-bravo", priority: "normal", preferred_protocol: "mqtt", secondary_protocol: "zenoh" },
        { node_id: "tower-gamma", priority: "normal", preferred_protocol: "mqtt", secondary_protocol: "zenoh" },
      ],
      relay_actions: [
        { node_id: "relay-01", assignment: "mesh-heal", target_zone: "bwari-north" },
        { node_id: "relay-02", assignment: "mesh-heal", target_zone: "bwari-east" },
      ],
    };
  }

  function simNodeRegistry() {
    return {
      cluster_id: "uriel-bwari-alpha",
      nodes: [
        { node_id: "tower-alpha", role: "fixed_tower", zone: "bwari-north", protocols: ["dds", "mqtt", "zenoh"], learning_layers: ["sl", "usl"], capabilities: ["optical", "thermal", "radar"], active: true, last_seen_ms: Date.now() - 8000 },
        { node_id: "tower-bravo", role: "fixed_tower", zone: "bwari-east", protocols: ["mqtt", "zenoh"], learning_layers: ["sl"], capabilities: ["optical", "radar"], active: true, last_seen_ms: Date.now() - 12000 },
        { node_id: "tower-gamma", role: "fixed_tower", zone: "bwari-south", protocols: ["mqtt"], learning_layers: ["sl"], capabilities: ["optical"], active: true, last_seen_ms: Date.now() - 22000 },
        { node_id: "orch-beta", role: "regional_hub", zone: "bwari-central", protocols: ["dds", "mqtt", "zenoh", "amqp"], learning_layers: ["sl", "usl", "rl"], capabilities: ["optical", "thermal", "radar", "manual"], active: true, last_seen_ms: Date.now() - 4000 },
        { node_id: "relay-01", role: "relay", zone: "bwari-mid", protocols: ["zenoh", "mqtt"], learning_layers: ["rl"], capabilities: ["radar"], active: true, last_seen_ms: Date.now() - 18000 },
        { node_id: "relay-02", role: "relay", zone: "bwari-south-mid", protocols: ["zenoh"], learning_layers: ["rl"], capabilities: ["radar"], active: false, last_seen_ms: Date.now() - 120000 },
      ],
    };
  }

  function simGovernanceAudit() {
    return [
      { timestamp_ms: Date.now() - 120000, cluster_id: "uriel-bwari-alpha", regional_summary: { active_nodes: 5, active_tracks: 80, dominant_threat_level: "monitor" }, policy_digest: { high_priority_protocol: "dds" }, federated_round: state.sim.fedRound - 2 },
      { timestamp_ms: Date.now() - 240000, cluster_id: "uriel-bwari-alpha", regional_summary: { active_nodes: 5, active_tracks: 72, dominant_threat_level: "none" }, policy_digest: { high_priority_protocol: "mqtt" }, federated_round: state.sim.fedRound - 5 },
    ];
  }

  // ── SIMULATION ADVANCE ─────────────────────────────────────────────────────
  function advanceSim() {
    if (!state.simWarned) {
      console.warn("[caesar-api] ⚠ WARNING: Hardware connection absent or incomplete. Using fallback SIMULATION data to populate the dashboard.");
      state.simWarned = true;
    }
    const s = state.sim;
    s.trackCount   = drift(s.trackCount,   3,    40,  220);
    s.throughput   = drift(s.throughput,   0.08, 0.3, 3.5);
    s.anomalyProb  = drift(s.anomalyProb,  0.004,0.01, 0.15);
    s.alignment    = drift(s.alignment,    0.005, 0.6, 1.0);
    s.fedRound     = Math.min(1000, s.fedRound + (Math.random() > 0.6 ? 1 : 0));
    s.fedLoss      = Math.max(0.0001, s.fedLoss * (0.998 + Math.random() * 0.005));
    s.soilMoisture = drift(s.soilMoisture, 0.6, 30, 95);
    s.ndvi         = drift(s.ndvi,         0.005, 0.3, 1.0);
    s.humidity     = drift(s.humidity,     0.4, 45, 99);
    s.pressure     = drift(s.pressure,     0.3, 8,  20);
    s.vibration    = Math.max(0.001, Math.abs(Math.sin(Date.now() / 4000)) * 0.03 + Math.random() * 0.006);
    s.gridConn     = drift(s.gridConn,     0.08, 95, 99.9);
    s.astEff       = drift(s.astEff,       0.3, 70, 99);
    s.waterSavings = drift(s.waterSavings, 80, 8000, 20000);
    s.vtolBat      = Math.max(5, s.vtolBat - 0.005 + Math.random() * 0.002);
    s.swarmUnits   = drift(s.swarmUnits,   1.5, 80, 200);
  }

  // ── EMIT ALL EVENTS (real or sim data) ────────────────────────────────────
  function broadcastData(fromServer) {
    const stats    = (fromServer && state.stats)           || simStats();
    const latest   = (fromServer && state.latest)          || simLatest();
    const hi       = (fromServer && state.highInterest)    || simHighInterest();
    const reg      = (fromServer && state.regionalSummary) || simRegionalSummary();
    const lp       = (fromServer && state.learningPlan)    || simLearningPlan();
    const orch     = (fromServer && state.orchestration)   || simOrchestration();
    const nr       = (fromServer && state.nodeRegistry)    || simNodeRegistry();
    const ga       = (fromServer && state.governanceAudit) || simGovernanceAudit();

    // Inject sim-enriched fields into real stats if server online but stats missing agri/infra fields
    if (fromServer && state.stats) {
      const s = state.sim;
      stats.soil_moisture_pct      = stats.soil_moisture_pct      ?? +s.soilMoisture.toFixed(1);
      stats.ndvi_index             = stats.ndvi_index             ?? +s.ndvi.toFixed(3);
      stats.humidity_pct           = stats.humidity_pct           ?? +s.humidity.toFixed(1);
      stats.max_yield_t_ha         = stats.max_yield_t_ha         ?? +(7.5 + s.soilMoisture*0.02 + s.ndvi*1.2).toFixed(2);
      stats.ast_efficiency_pct     = stats.ast_efficiency_pct     ?? +s.astEff.toFixed(0);
      stats.water_savings_l_wk     = stats.water_savings_l_wk     ?? Math.round(s.waterSavings);
      stats.swarm_units            = stats.swarm_units            ?? Math.round(s.swarmUnits);
      stats.vtol_battery_pct       = stats.vtol_battery_pct       ?? +s.vtolBat.toFixed(0);
      stats.system_integrity_pct   = stats.system_integrity_pct   ?? +(98.5 + s.alignment*1.3).toFixed(2);
      stats.grid_connectivity_pct  = stats.grid_connectivity_pct  ?? +s.gridConn.toFixed(1);
      stats.pipeline_pressure_bar  = stats.pipeline_pressure_bar  ?? +s.pressure.toFixed(1);
      stats.vibration_rms          = stats.vibration_rms          ?? +s.vibration.toFixed(3);
    }

    emit("caesar:stats",            stats);
    emit("caesar:latest",           latest);
    emit("caesar:high-interest",    hi);
    emit("caesar:regional-summary", reg);
    emit("caesar:learning-plan",    lp);
    emit("caesar:orchestration",    orch);
    emit("caesar:node-registry",    nr);
    emit("caesar:governance-audit", ga);
    emit("caesar:tick", {
      online: state.online,
      lastSyncMs: state.lastSuccessMs,
      stale: stats.stale || false,
      state: { ...state.sim },
    });
  }

  // ── POLL SERVER ────────────────────────────────────────────────────────────
  async function pollAll() {
    state.lastAttemptMs = Date.now();

    // Quick health check first
    const health = await fetchJSON("/healthz");
    if (!health) {
      if (state.online) {
        state.online = false;
        emit("caesar:offline", { failures: ++state.consecutiveFailures });
      } else {
        state.consecutiveFailures++;
      }
      advanceSim();
      broadcastData(false);
      return;
    }

    // Server alive — fetch everything in parallel
    const EMPTY_STATS = {
      activity_window_seconds: 900, active_cutoff_ms: Date.now()-900000,
      latest_track_count:0, high_interest_recent_count:0, active_high_interest_count:0,
      node_counts:{}, threat_counts:{}, modality_counts:{}, site_counts:{},
      registered_node_count:0, active_node_count:0, throughput_events_per_min:0,
      anomaly_probability:0, node_health_ratio:0, fed_alignment:0,
      federated_participant_count:0, recent_journal_count:0, last_detection_ms:null, stale:true,
    };

    const [stats, latest, hi, reg, lp, orch, nr, ga] = await Promise.all([
      fetchJSON("/api/stats",                EMPTY_STATS),
      fetchJSON("/api/latest",               {}),
      fetchJSON("/api/high-interest?limit=40", []),
      fetchJSON("/api/regional-summary",     { region:"Bwari, FCT" }),
      fetchJSON("/api/learning-plan",        { supervised_learning:[], reinforcement_learning:[], semi_supervised_learning:[], federated_round:{} }),
      fetchJSON("/api/orchestration",        { policy_digest:{}, routing_actions:[], relay_actions:[] }),
      fetchJSON("/api/node-registry",        { nodes:[], cluster_id:"uriel orchestrator" }),
      fetchJSON("/api/governance-audit?limit=10", []),
    ]);

    // Store
    state.stats           = stats;
    state.latest          = latest;
    state.highInterest    = Array.isArray(hi) ? hi : [];
    state.regionalSummary = reg;
    state.learningPlan    = lp;
    state.orchestration   = orch;
    state.nodeRegistry    = nr;
    state.governanceAudit = Array.isArray(ga) ? ga : [];

    // Sync sim to real data so charts don't jump if we go offline
    if (stats.latest_track_count)     state.sim.trackCount   = stats.latest_track_count;
    if (stats.throughput_events_per_min) state.sim.throughput = stats.throughput_events_per_min;
    if (stats.anomaly_probability)    state.sim.anomalyProb  = stats.anomaly_probability;
    if (stats.fed_alignment)          state.sim.alignment    = stats.fed_alignment;
    if (lp.federated_round?.round_id) state.sim.fedRound     = lp.federated_round.round_id;

    const wasOffline = !state.online;
    state.online = true;
    state.consecutiveFailures = 0;
    state.lastSuccessMs = Date.now();
    state.simWarned = false;

    if (wasOffline) emit("caesar:online", { lastSuccessMs: state.lastSuccessMs });

    advanceSim();        // still advance sim for fields server doesn't provide
    broadcastData(true);
  }

  // ── BOOT ──────────────────────────────────────────────────────────────────
  // Initial sim broadcast immediately (dashboards render at once)
  advanceSim();
  broadcastData(false);

  // Start polling
  setInterval(pollAll, POLL_MS);
  pollAll(); // first real poll

  // Extra sim ticks between real polls so animations stay smooth
  setInterval(() => {
    if (!state.online) {
      advanceSim();
      broadcastData(false);
    }
  }, SIM_MS);

  // Expose read-only state for debugging
  global.caesarAPI = {
    getState: () => ({ ...state }),
    getOnline: () => state.online,
    forceRefresh: () => pollAll(),
  };

  // ── SERVER-SENT EVENTS ── (primary real-time push channel)
  function connectSSE() {
    if (!global.EventSource) return;
    const es = new global.EventSource(`${BASE}/api/live-events`);
    es.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.stats)  { state.stats  = data.stats;  emit("caesar:stats",  data.stats);  }
        if (data.latest) { state.latest = data.latest; emit("caesar:latest", data.latest); }
        if (!state.online) { state.online = true; state.consecutiveFailures = 0; emit("caesar:online", {}); }
        state.lastSuccessMs = Date.now();
    state.simWarned = false;
        emit("caesar:tick", { online: true, lastSyncMs: state.lastSuccessMs, stale: false, state: { ...state.sim } });
      } catch (_) {}
    };
    es.onerror = () => { setTimeout(connectSSE, 5000); es.close(); };
  }
  connectSSE();

})(window);
