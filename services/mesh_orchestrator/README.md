# Mesh Orchestrator

This service fills in the multi-node and learning-fabric layers described in the project documents.

It reads Caesar hub outputs and cluster configuration, then writes:

- `node_registry.json`
- `regional_summary.json`
- `orchestration_plan.json`
- `learning_plan.json`
- `governance_audit.jsonl`

## Run

```powershell
python services/mesh_orchestrator/orchestrator.py --run-once
```
