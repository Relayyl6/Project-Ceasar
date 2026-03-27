# Caesar Console

This service exposes a simple API and operator dashboard over the Caesar hub files.

## Run

```powershell
python services/caesar_console/server.py --host 127.0.0.1 --port 8090
```

## Endpoints

- `/healthz`
- `/api/latest`
- `/api/high-interest?limit=25`
- `/api/journal?limit=100`
- `/api/stats`
- `/api/regional-summary`
- `/api/orchestration`
- `/api/learning-plan`
- `/api/node-registry`

Open `http://127.0.0.1:8090/` in a browser for the dashboard.
