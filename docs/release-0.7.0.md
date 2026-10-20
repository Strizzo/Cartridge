# CartridgeOS 0.7.0 — public app refresh

Nine bundled apps now share larger typography, original app icons, a graphite/paper palette and clear controller hints: Calculator, Tasks, Pomodoro, Weather, System Monitor, Net Tool, Hacker News, AI Papers and Stocks. Each keeps a layout suited to its content rather than using the launcher grid inside every app. Idle views do not continuously animate.

Calculator keeps its evaluator and history. Tasks keeps priorities and completed items. Pomodoro retains timer settings and statistics, and skipping a phase no longer counts it as a completed session. Reading apps have quieter lists, readable detail pages and explicit loading/error states. Stocks preserves a removed watchlist symbol across delayed responses and keeps chart history with its matching period. System Monitor labels simulator values and reports aggregate CPU data instead of invented per-core values.

The Frequency bundle now matches the signed Store release 1.2.0, including the reported-location atlas and independent stick controls. Mission Control and Outside retain their existing public releases.

VibeBoy 0.4.0 uses the same app components and requires runtime 0.7.0. Its release is included in the bundle and signed Store. World Pulse 0.3.0 is a separate development preview; Store publication is held pending live GDELT verification. VibeBoy opens SSH tunnels asynchronously, remains responsive during connection, and ignores cancelled results. World Pulse separates news, geographic and ISS source failures, marks cached results and never substitutes fabricated live data. Provider checks found the geographic endpoint returning 404 and the news endpoint returning 429; ISS responded. External service availability remains visible in the preview.

Update a bootstrapped device from **Settings → System Update**, restart, then refresh Store with Y. The update uses the existing trusted verifier and supervisor. App preferences and game saves remain outside the release payload.

Validation includes Rust regression tests, native 720×720 captures with deterministic service fixtures, package validation and ARM Linux compatibility/OTA rollback checks. Software simulation covers application behavior and installation; physical GPU, radio, audio and battery performance still need a handheld check.
