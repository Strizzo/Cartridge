# CartridgeOS Installer for macOS

`bash mac-installer/build.sh` builds a native SwiftUI app. By default it appears
in the temporary directory as `CartridgeOS Installer.app`; pass an absolute app
path as the first argument to choose another location.

This first installer supports **a verified local first-boot sparsebundle and an
empty, removable exFAT card of exactly the same size**. The image and its
`firstboot-image-report.json` must be together on a mounted external drive.
The app inventories disks read-only, requires an explicit target selection,
checks the card is empty, then requires a separate acknowledgment before any
raw write. It compares a full-card readback against the source image and ejects
only after verification. A card that macOS ejects during writing must be
reinserted and verified before use.

Cards containing games are displayed for identification but cannot be written
by this version of the app. The existing command-line preserve workflow is
separate. This app does not build a distributable OS image or download one.

The app runs `installer/mac_bridge.py`, which delegates to the tested device
image preflight, writer, and readback scripts. Only the selected disk can be
held unmounted by the bundled Disk Arbitration guard during writing.
