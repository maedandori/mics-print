# MICS Print Rust

Imported the existing two-EXE Rust Print Agent from maedandori/fujifilm-print and added an optional background MICS Firebird reader based on maedandori/mics-data.

Print Agent retains Excel/PDF, SumatraPDF, WebSocket, heartbeat, auto-update and Windows System Tray. Print Settings is retained.

## MICS
Copy mics.ini.example to mics.ini. Set enabled=1 only on the one Windows PC authorized to connect to the Firebird server; other agents must keep it disabled. Configure MICS-Credentials.exe (encrypted Windows DPAPI vault). Restart the agent after changing environment variables.

Read-only MACHINEWORK + SCHEDULE + TOOL query returns current status, order, mold, product, planned quantity, OK/NG quantity and cycle time. No employee names are uploaded. Data is sampled every 30 seconds and HTTPS-uploaded only when changed, using the same JSON fields as mics-data. Worker endpoint and D1 tables must already exist.

The single-reader setting is not a distributed lock. For strict enforcement with two agents a Worker lease is needed. Current MACHINEWORK snapshots do not provide a full 07:30 shift changeover history.

New updater targets maedandori/mics-print Releases, never fujifilm-print. GitHub Actions runs Windows tests and builds; publishing a Release is manual for the first version. Do not use same Agent ID simultaneously with the old printing app. Validate the CI build and PDF printing before production.

## Secure credentials (Windows)
Run **MICS-Credentials.exe** under the same Windows account that runs Print-Agent.exe. Enter Firebird database password and MICS_SYNC_TOKEN in masked inputs; the program stores a Windows DPAPI CurrentUser-encrypted vault at `%LOCALAPPDATA%\\MICS-Print\\credentials.dpapi`.

The Agent reads this encrypted vault in-process when MICS sync is enabled. It does not launch a command-line helper, expose credentials through stdout, write secrets into mics.ini, or require environment variables. DPAPI is scoped to the current Windows user on the local machine. A different Windows account or another PC must set up its own vault. An attacker already running code as this user can potentially decrypt the vault; use a dedicated Windows account and least-privilege Firebird user.

New releases include three EXEs: Print-Agent.exe, Print-Settings.exe, and MICS-Credentials.exe. The vault is stored outside the installation directory and never included in GitHub releases. Restart Print Agent after saving credentials.
