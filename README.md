# MICS Print Rust

Imported the existing two-EXE Rust Print Agent from maedandori/fujifilm-print and added an optional background MICS Firebird reader based on maedandori/mics-data.

Print Agent retains Excel/PDF, SumatraPDF, WebSocket, heartbeat, auto-update and Windows System Tray. Print Settings is retained.

## MICS
Copy mics.ini.example to mics.ini. Set enabled=1 only on the one Windows PC authorized to connect to the Firebird server; other agents must keep it disabled. Enter Firebird password and MICS_SYNC_TOKEN in the MICS tab of Print-Settings.exe and save. Restart the agent.

Read-only MACHINEWORK + SCHEDULE + TOOL query returns current status, order, mold, product, planned quantity, OK/NG quantity and cycle time. The read-only MICS snapshot now includes MACHINEWORK.STAFFNAME as `staff_name` when available. Treat this as employee data; restrict access to the Worker/D1 and avoid logging raw names. Data is sampled every 30 seconds and HTTPS-uploaded only when changed, using the same JSON fields as mics-data. Worker endpoint and D1 tables must already exist. The MICS tab includes **Test đồng bộ MICS**, which performs one Firebird read and one HTTPS POST even if the snapshot is unchanged; it reports Firebird row counts, populated STAFFNAME count and Worker HTTP acceptance. It does not prove D1 column persistence without a separate D1 readback.

The single-reader setting is not a distributed lock. For strict enforcement with two agents a Worker lease is needed. Current MACHINEWORK snapshots do not provide a full 07:30 shift changeover history.

New updater targets maedandori/mics-print Releases, never fujifilm-print. GitHub Actions runs Windows tests and builds; publishing a Release is manual for the first version. Do not use same Agent ID simultaneously with the old printing app. Validate the CI build and PDF printing before production.

## Secure credentials (Windows)
Run **Print-Settings.exe**, open the **MICS tab**, and enter secrets under the same Windows account that runs Print-Agent.exe. Enter Firebird database password and MICS_SYNC_TOKEN in masked inputs; the program stores a Windows DPAPI CurrentUser-encrypted vault at `%LOCALAPPDATA%\\MICS-Print\\credentials.dpapi`.

The Agent reads this encrypted vault in-process when MICS sync is enabled. It does not launch a command-line helper, expose credentials through stdout, write secrets into mics.ini, or require environment variables. DPAPI is scoped to the current Windows user on the local machine. A different Windows account or another PC must set up its own vault. An attacker already running code as this user can potentially decrypt the vault; use a dedicated Windows account and least-privilege Firebird user.

New releases include two EXEs: Print-Agent.exe and Print-Settings.exe. The vault is stored outside the installation directory and never included in GitHub releases. Restart Print Agent after saving credentials.

## Staged ACTIVE/STANDBY Firebird handover (not yet activated in production)
`src/production/lease.rs` adds an **optional** coordinator lease shared by both the machine and schedule reader threads. Print and PDF jobs are unaffected.
- `mics.ini` default `failover_enabled=0`; with flag OFF legacy MICS sync continues as before.
- Set `coordinator_url=https://machine-control.maedandoris.workers.dev/api/mics-failover` and `failover_enabled=1` **only after** upgrading the primary Windows agent, installing the updated backup agent and verifying the Worker coordinator API.
- Primary uses the existing `MICS_SYNC_TOKEN` from its per-user DPAPI vault to request a `role=primary` lease. The unique primary agent ID comes from `agent-id.txt`, not a shared hardcoded ID.
- The machine snapshot, weekly schedule, and manual MICS test are all lease-gated when enabled; old unleased Worker routes will reject writes while Cloudflare's `MICS_FAILOVER_ENABLED=1`.
- If primary Firebird reads fail, the primary attempts to release its lease and backs off. A returning primary signals the backup to stop renewal; takeover occurs after the backup's short lease expires, not by simultaneous use of Firebird.
- This is a staged protocol, not proof of production-ready zero-overlap: verify Windows binary builds, coordinator deployment, both agents' logs and a deliberate disconnect/reconnect test before changing the Cloudflare flag.
- A standalone non-installing Windows ZIP artifact is built by `.github/workflows/failover-build.yml`. Do not overwrite working production executables until tests are successful.
