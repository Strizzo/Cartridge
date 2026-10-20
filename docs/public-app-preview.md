# Public app previews and recording

Use the native simulator for UI iteration without moving the SD card:

```sh
./sim.sh app calculator
./sim.sh app todo
./sim.sh app pomodoro
./sim.sh app weather
./sim.sh app hacker_news
./sim.sh app stock_market
./sim.sh app ../vibeboy-cartridge
./sim.sh app ../worldpulse-cartridge
```

Keyboard: arrows are the D-pad; Z=A, X=B, C=X, V=Y; A/S=L1/R1, Q/W=L2/R2, Return=Start, Space=Select. A connected controller uses the shared input path. Screen content is 720×720; window scaling is a simulator setting and does not change the handheld layout.

For recording, Calculator history, the Tasks register, Pomodoro timer/settings, Weather forecast and Stocks detail chart show the new layouts well. Hacker News and AI Papers have readable list and reader views. VibeBoy requires the companion daemon and your own server; simulator fixture captures do not send actions to a real session. World Pulse shows upstream source status and cached-data labels, including when a provider is unavailable.

`app-check` runs against an isolated temporary home. Supply `--fixture FILE`, `--seed KEY=JSON`, `--press FRAME:BUTTON`, `--dt SECONDS`, `--capture N,N` and `--out DIR` for reproducible capture sequences. Fixtures are test data, so caption them accordingly in public videos. Avoid recording Wi-Fi passwords, private keys, personal tasks or remote terminal secrets.

Static views should remain idle between changes. Compare rendered-frame counts from `app-check` before adding animation. Native Mac timing is useful for relative regressions; it is not a Cortex-A35 performance measurement.
