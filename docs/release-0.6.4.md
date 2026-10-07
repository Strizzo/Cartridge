# CartridgeOS 0.6.4

Store updates now report the result clearly. A fast failure appears before older
catalogue-refresh notices and stays visible until dismissed with R2. Retrying
keeps one copy of an error and returns its text to the first page. Successful
installation appears before older success notices; unread errors remain available.
Store progress is shown while a worker is running, and R2 cannot dismiss an
unseen result during that task.

Before starting a download, Store checks the app's minimum CartridgeOS version.
Frequency 1.2.0 requires 0.6.2 or newer. An older runtime explains that the user
must open Settings → System Update, install the OS update and restart, then retry
the app update. The installer still enforces the same requirement independently.
The app's detail screen also displays an incompatible runtime requirement.

On a device with the trusted 0.6.1 updater bootstrap, check, download and restart
from Settings → System Update. Then refresh Store and update Frequency.
This release uses the existing signature and supervisor protocol. It does not
change the Linux kernel, drivers, boot configuration, games, saves or app data.
