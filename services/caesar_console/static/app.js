async function loadJson(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Request failed: ${path}`);
  }
  return response.json();
}

function renderStatList(target, items) {
  target.innerHTML = "";
  const entries = Object.entries(items);
  if (!entries.length) {
    target.innerHTML = "<p class='empty'>No data yet.</p>";
    return;
  }
  for (const [label, value] of entries) {
    const row = document.createElement("div");
    row.className = "stat-row";
    row.innerHTML = `<span>${label}</span><strong>${value}</strong>`;
    target.appendChild(row);
  }
}

function formatLabel(label) {
  return label.replaceAll("_", " ");
}

function renderTracks(target, latest) {
  target.innerHTML = "";
  const records = Object.values(latest);
  if (!records.length) {
    target.innerHTML = "<tr><td colspan='5' class='empty-cell'>No tracks yet.</td></tr>";
    return;
  }
  for (const record of records.sort((a, b) => b.received_at_ms - a.received_at_ms)) {
    const body = record.envelope.body;
    const row = document.createElement("tr");
    row.innerHTML = `
      <td>${body.track_id}</td>
      <td>${body.node_id}</td>
      <td><span class="pill ${body.threat_level}">${body.threat_level}</span></td>
      <td>${body.confidence.toFixed(3)}</td>
      <td>${body.contributing_modalities.join(", ")}</td>
    `;
    target.appendChild(row);
  }
}

function renderAlerts(target, alerts) {
  target.innerHTML = "";
  if (!alerts.length) {
    target.innerHTML = "<p class='empty'>No high-interest alerts yet.</p>";
    return;
  }
  for (const record of alerts) {
    const body = record.envelope.body;
    const card = document.createElement("article");
    card.className = "alert-card";
    card.innerHTML = `
      <header>
        <strong>${body.track_id}</strong>
        <span>${body.site}</span>
      </header>
      <p>${body.threat_level} at confidence ${body.confidence.toFixed(3)}</p>
      <small>${body.node_id} | ${body.contributing_modalities.join(", ")}</small>
    `;
    target.appendChild(card);
  }
}

function renderNodeRegistry(target, registry) {
  target.innerHTML = "";
  const nodes = registry.nodes || [];
  if (!nodes.length) {
    target.innerHTML = "<p class='empty'>No node registry data yet.</p>";
    return;
  }
  for (const node of nodes) {
    const card = document.createElement("article");
    card.className = "registry-card";
    card.innerHTML = `
      <header>
        <strong>${node.node_id}</strong>
        <span class="pill ${node.active ? "active-pill" : "inactive-pill"}">${node.active ? "active" : "inactive"}</span>
      </header>
      <p>${formatLabel(node.role)} in ${node.zone}</p>
      <small>Protocols: ${node.protocols.join(", ")}</small>
      <small>Learning: ${node.learning_layers.join(", ")}</small>
      <small>Capabilities: ${node.capabilities.join(", ")}</small>
    `;
    target.appendChild(card);
  }
}

function renderSummary(target, items) {
  target.innerHTML = "";
  for (const [label, value] of Object.entries(items)) {
    const row = document.createElement("div");
    row.className = "stat-row";
    row.innerHTML = `<span>${label}</span><strong>${value}</strong>`;
    target.appendChild(row);
  }
}

function renderOrchestration(target, plan) {
  target.innerHTML = "";
  const actions = [...(plan.routing_actions || []), ...(plan.relay_actions || [])];
  if (!actions.length) {
    target.innerHTML = "<p class='empty'>No orchestration plan yet.</p>";
    return;
  }
  for (const action of actions) {
    const card = document.createElement("article");
    card.className = "alert-card";
    const details = Object.entries(action)
      .map(([key, value]) => `${key}: ${value}`)
      .join(" | ");
    card.innerHTML = `<p>${details}</p>`;
    target.appendChild(card);
  }
}

function renderGovernance(targetSummary, targetAudit, stats, registry, audit) {
  renderSummary(targetSummary, {
    registered_nodes: stats.registered_node_count || 0,
    active_nodes: Object.keys(stats.node_counts || {}).length,
    trust_posture: stats.registered_node_count > 0 ? "allowlisted" : "open",
    audit_events: audit.length,
  });

  targetAudit.innerHTML = "";
  if (!audit.length) {
    targetAudit.innerHTML = "<p class='empty'>No governance audit events yet.</p>";
    return;
  }

  for (const event of audit) {
    const card = document.createElement("article");
    card.className = "alert-card";
    card.innerHTML = `
      <p><strong>Federated round:</strong> ${event.federated_round}</p>
      <small>policy digest: ${Object.values(event.policy_digest || {}).join(", ")}</small>
    `;
    targetAudit.appendChild(card);
  }
}

function renderLearningPlanDetails(target, learningPlan) {
  target.innerHTML = "";
  const sections = [
    ["Supervised", learningPlan.supervised_learning || []],
    ["Semi-supervised", learningPlan.semi_supervised_learning || []],
    ["Reinforcement", learningPlan.reinforcement_learning || []],
  ];

  for (const [title, jobs] of sections) {
    const card = document.createElement("article");
    card.className = "alert-card";
    if (!jobs.length) {
      card.innerHTML = `<header><strong>${title}</strong></header><p>No jobs planned.</p>`;
    } else {
      const items = jobs
        .map((job) => `<li>${job.node_id}: ${formatLabel(job.job_type)} -> ${job.target_model || job.target_policy}</li>`)
        .join("");
      card.innerHTML = `<header><strong>${title}</strong></header><ul class="compact-list">${items}</ul>`;
    }
    target.appendChild(card);
  }
}

function buildMissionNarrative(stats, regionalSummary, learningPlan, registry) {
  const activeNodes = Object.keys(stats.node_counts || {}).length;
  const profiles = [];
  if (activeNodes) profiles.push(`${activeNodes} live node${activeNodes === 1 ? "" : "s"}`);
  if (regionalSummary.active_track_count) profiles.push(`${regionalSummary.active_track_count} active track${regionalSummary.active_track_count === 1 ? "" : "s"}`);
  if ((learningPlan.supervised_learning || []).length) profiles.push(`${(learningPlan.supervised_learning || []).length} supervised updates queued`);
  if ((learningPlan.reinforcement_learning || []).length) profiles.push(`${(learningPlan.reinforcement_learning || []).length} routing policy jobs`);
  if (!profiles.length) {
    return "The mesh is online but quiet: node registry, learning fabric, and governance layers are ready to coordinate once edge nodes begin sending semantic tracks.";
  }

  return `The mesh is currently coordinating ${profiles.join(", ")}. This view ties edge sensing to the regional story: agriculture and infrastructure operators see corridor and anomaly pressure, surveillance teams see node posture and alert density, and tactical users see which relay and protocol layers should carry high-priority traffic next.`;
}

async function refresh() {
  const [stats, latest, alerts, regionalSummary, learningPlan, orchestrationPlan, nodeRegistry, governanceAudit] = await Promise.all([
    loadJson("/api/stats"),
    loadJson("/api/latest"),
    loadJson("/api/high-interest?limit=25"),
    loadJson("/api/regional-summary"),
    loadJson("/api/learning-plan"),
    loadJson("/api/orchestration"),
    loadJson("/api/node-registry"),
    loadJson("/api/governance-audit?limit=10"),
  ]);

  document.getElementById("latestTrackCount").textContent = stats.latest_track_count;
  document.getElementById("highInterestCount").textContent = stats.high_interest_recent_count;
  document.getElementById("nodeCount").textContent = Object.keys(stats.node_counts).length;
  document.getElementById("registeredNodeCount").textContent = stats.registered_node_count;
  document.getElementById("missionNarrative").textContent = buildMissionNarrative(
    stats,
    regionalSummary,
    learningPlan,
    nodeRegistry,
  );

  renderStatList(document.getElementById("threatCounts"), stats.threat_counts);
  renderStatList(document.getElementById("nodeCounts"), stats.node_counts);
  renderSummary(document.getElementById("regionalSummary"), {
    active_nodes: regionalSummary.active_node_count || 0,
    active_tracks: regionalSummary.active_track_count || 0,
    dominant_threat: regionalSummary.dominant_threat_level || "none",
    recent_high_interest: regionalSummary.high_interest_recent_count || 0,
  });
  renderSummary(document.getElementById("learningSummary"), {
    supervised_jobs: (learningPlan.supervised_learning || []).length,
    semi_supervised_jobs: (learningPlan.semi_supervised_learning || []).length,
    reinforcement_jobs: (learningPlan.reinforcement_learning || []).length,
    federated_strategy: learningPlan.federated_round ? learningPlan.federated_round.strategy : "none",
  });
  renderTracks(document.getElementById("latestTracks"), latest);
  renderAlerts(document.getElementById("alerts"), alerts);
  renderOrchestration(document.getElementById("orchestrationPlan"), orchestrationPlan);
  renderNodeRegistry(document.getElementById("nodeRegistry"), nodeRegistry);
  renderGovernance(
    document.getElementById("governanceSummary"),
    document.getElementById("governanceAudit"),
    stats,
    nodeRegistry,
    governanceAudit,
  );
  renderLearningPlanDetails(document.getElementById("learningPlanDetails"), learningPlan);
}

document.getElementById("refresh").addEventListener("click", refresh);
refresh().catch((error) => {
  console.error(error);
});
setInterval(() => {
  refresh().catch((error) => console.error(error));
}, 5000);
