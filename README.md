# MICS Print Rust

Imported the existing two-EXE Rust Print Agent from maedandori/fujifilm-print and added an optional background MICS Firebird reader based on maedandori/mics-data.

Print Agent retains Excel/PDF, SumatraPDF, WebSocket, heartbeat, auto-update and Windows System Tray. Print Settings is retained.

## MICS
Copy mics.ini.example to mics.ini. Set enabled=1 only on the one Windows PC authorized to connect to the Firebird server; other agents must keep it disabled. Configure Windows environment variables MICS_DB_PASSWORD and MICS_SYNC_TOKEN. Restart the agent after changing environment variables.

Read-only MACHINEWORK + SCHEDULE + TOOL query returns current status, order, mold, product, planned quantity, OK/NG quantity and cycle time. No employee names are uploaded. Data is sampled every 30 seconds and HTTPS-uploaded only when changed, using the same JSON fields as mics-data. Worker endpoint and D1 tables must already exist.

The single-reader setting is not a distributed lock. For strict enforcement with two agents a Worker lease is needed. Current MACHINEWORK snapshots do not provide a full 07:30 shift changeover history.

New updater targets maedandori/mics-print Releases, never fujifilm-print. GitHub Actions runs Windows tests and builds; publishing a Release is manual for the first version. Do not use same Agent ID simultaneously with the old printing app. Validate the CI build and PDF printing before production.
