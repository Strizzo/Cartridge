# Experimental GBM compatibility investigation

The static probe is usable now. The guard is an opt-in experiment, **not evidence
of working hardware GPU rendering**. Nothing here changes the engine wrapper,
Chromium options, services, host libraries, permissions, or kernel. Keep the
normal engine's GPU setting unchanged for a persistent release.

## Exact target result

`observed-rg351mp.json` records a read-only run of `abi_probe.py` on 2026-10-27:
RG351MP/RK3326, Linux 4.4.189, Debian Chromium 156.0.8078.47-1~deb12u1, and the
GBM vendor library actually mapped by SDL:

```
/usr/local/lib/aarch64-linux-gnu/libmali-bifrost-g31-rxp0-gbm.so
```

Both files are AArch64. The vendor library lacks these **strong** Chromium imports:

```
gbm_bo_create_with_modifiers    gbm_bo_get_modifier
gbm_bo_get_handle_for_plane     gbm_bo_get_offset
gbm_bo_get_plane_count          gbm_bo_get_stride_for_plane
gbm_bo_map                      gbm_bo_unmap
```

`gbm_bo_get_fd_for_plane` is separately missing but **weak/optional**; Chromium
has a real DRM export fallback for that function. Stubbing the eight required
operations cannot make the legacy buffer representation compatible.

The driver provides legacy opaque BO/device/surface operations, but no public
memory-plane, modifier, or mapping APIs. A packed RGB format does not prove a
single memory plane or linear layout. In particular, returning plane count 1,
offset 0, or modifier LINEAR would fabricate facts about allocations. Inspecting
private vendor structs or calculating strides/offsets is deliberately excluded.

Chromium calls its metadata getters without a reliable error path after creating
a BO. Therefore the guard rejects creation/import/acquisition **before returning
a BO or surface** when its required buffer API is absent. Returning an error
sentinel only from the later plane getter is insufficient.

Primary references:

- [Chromium GBM consumer](https://chromium.googlesource.com/chromium/src/+/HEAD/ui/gfx/linux/gbm_wrapper.cc), especially `CreateBufferForBO`, `GetSizeOfPlane`, and `GetPlaneFdForBo`.
- [Public GBM ABI declarations](https://chromium.googlesource.com/chromiumos/platform/minigbm/+/refs/heads/master/gbm.h).
- [Headless GPU constraints](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/gpu/using-gpu-hardware-in-headless-chrome.md).

## Static probe: no driver loading or device access

Python 3.7+, no dependencies or `readelf` required:

```sh
python3 abi_probe.py --backend /path/to/vendor-gbm.so \
    --consumer /path/to/chromium
```

The program reads regular ELF64 little-endian files. It refuses special files,
does not invoke subprocesses, and never loads a library or starts Chromium.
It handles dynamic imports/exports, weak imports, visibility, architecture,
dependency names, and bounds-checks the ELF tables it reads. GNU symbol versions
and dependency resolution are explicitly **not** validated.

Exit codes: 0 = necessary name/architecture checks pass; 1 = incompatible symbol
coverage/architecture (the observed vendor result); 2 = invalid/unsupported input.
JSON always says `gpu_compatibility_established: false` on a valid comparison.
Passing symbol checks cannot establish GPU compatibility, EGL initialization,
buffer interoperability, or sandbox operation. The probe also does not inspect
definitions supplied by other dependencies: it assesses the selected provider.

## Guard contract

`gbm_guard.c` exports the observed Chromium imports and standard legacy GBM calls.
`gbm_api.h` contains opaque public ABI declarations, not vendor implementations.

- There is no implicit backend. `CARTRIDGE_GBM_BACKEND` must name an absolute
  regular-file path to one explicitly selected, trusted library inside the
  private sandbox. Initialization is lazy and runs once per process.
- `dlopen` uses `RTLD_NOW | RTLD_LOCAL`. Each function must come from that exact
  backend object, not another dependency or the guard itself. This prevents
  mixing opaque Mesa/vendor BO implementations through `dlsym` dependency lookup.
- Device and existing legacy queries forward unchanged backend values. BO
  creation, import, surface creation, front-buffer acquisition and positive
  format capability reports require the full checked buffer API. Surface
  operations additionally require their destruction/release functions.
- Absent operations return `EOPNOTSUPP` with NULL, -1, invalid modifier, invalid
  handle, or an explicitly invalid offset as appropriate. Failed mapping clears
  its stride/map-data outputs. No planes, modifiers, mappings or descriptors are
  invented. Optional allocation-v2 calls are rejected rather than dropping flags.
- `cartridge_gbm_buffer_abi_covered()` reports only API coverage.
  `cartridge_gbm_backend_status()` returns `backend-unavailable`,
  `legacy-buffer-abi-rejected`, or `buffer-abi-covered; hardware-unproven`.
- The backend remains loaded while its opaque objects can exist. Objects from
  other GBM implementations must never be passed through this guard.

This is not a security boundary or a GPU driver. Loading a trusted vendor DSO can
run its constructors and driver code. The guard does not constrain driver-internal
EGL allocations. Actual vendor loading belongs only in the parent's bounded
sandbox trial; the static probe is the read-only diagnostic used on the device.

## Build and test

```sh
make all
make test
```

Build on aarch64 Linux for the engine. The output `build/libgbm.so.1` has that
SONAME and no vendor code. A Mac build exists solely to test the C forwarding
contract and is not a Linux engine binary. Build output is ignored by Git.

The tests use original mock libraries and no hardware. They verify legacy and
partial-ABI rejection before backend allocations, exact modern forwarding,
backend error propagation, callbacks, optional-function failures, map output
clearing, missing/relative/self backend rejection, concurrent initialization,
and dependency-provider isolation. ELF fixtures verify corrupt input handling,
special-file rejection, strong/weak distinction, and deliberately limited claims.

The original guard was also compiled and tested in the already-running Linux
aarch64 VM. This establishes userspace build/test behavior, not Linux 4.4 driver
behavior. No proprietary library is copied into this directory or redistributed.

## Parent integration and physical validation still required

The parent owns wrapper changes and actual trials. If trialing this guard, mount
only the original guard and the explicitly selected driver/dependencies into the
private userspace, keep all existing namespace/seccomp protections, and avoid
replacing host libraries or broad device/host-directory mounts. Explicitly pass
the backend path through the wrapper's cleared environment. Do not add loader
preloads, sandbox bypasses or browser identity changes.

The expected legacy result is `legacy-buffer-abi-rejected`: BO/import/surface
calls fail, rather than reporting a compatible layout. The guard may permit
testing whether headless vendor EGL/GLES can avoid Chromium's GBM allocation path;
that possibility remains physically unproven. The vendor EGL platform support,
private-userspace dependency compatibility, kernel driver ABI, GPU sandbox policy,
WebGL renderer, screenshot pixels, stability and thermal/power behavior all need
independent evidence. A symbol-loading success or an EGL context alone is not a
Chromium rendering acceptance result. There is no hardware video-decode claim.

If Chromium requires these GBM paths, the blocker is a backend that supplies real
plane/modifier/map operations, or a separate upstream-supported rendering path.
The present legacy exports do not provide enough data for a safe small layout
adapter. Keep this guard opt-in and out of the default signed-engine path.
