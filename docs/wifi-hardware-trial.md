# R36S Plus Wi-Fi hardware trial

The spare card's USB Wi-Fi device identifies as `0bda:0179`. Its Linux image
contains two matching drivers: staging `r8188eu` and Rockchip `8188eu`. With
both loaded, `r8188eu` bound `wlan0`; NetworkManager reported it unavailable.
Unloading that driver during a running session removed `wlan0` and did not
transfer it to the other driver.

A reversible change to the spare card's BOOT `boot.ini` adds
`modprobe.blacklist=r8188eu` for a clean-start test. That produced `wlan0` and
`p2p0` under `rtl8188eu`. Both were still unavailable to NetworkManager, and
`wpa_supplicant` failed at boot. Cartridge also selected `p2p0` for an access
point scan; the launcher now skips P2P interfaces and prefers an available
station interface. The service failure remains a separate issue.

`deploy/wifi/wpa-repair-trial.sh` is a device-only tool for the known spare card.
It first restarts the current WPA service and NetworkManager. If an actual
`wlan0` scan still finds no access points, it tries the standard WPA D-Bus
startup command with an override in `/run`. It installs a persistent drop-in
under `/etc/systemd/system/wpa_supplicant.service.d` only after a successful
access-point scan, then repeats the scan with the persistent configuration.
Failures remove the trial override. The original unit file is never replaced;
`deploy/wifi/undo-wpa-repair.sh` removes only the exact Cartridge drop-in.
The trial log is saved beside Cartridge on EASYROMS.

These are hardware diagnostics, not a general online update. No BOOT/kernel,
Linux-root or driver change belongs in the online updater until the handheld
passes a scan, connection and reboot test with a documented rollback.

The subsequent service trial failed and retained no override. The ARM VM
reproduced a corrupted WPA executable; see [the package recovery evidence and
procedure](wifi-package-recovery.md).
