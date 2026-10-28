/* Experimental opt-in ABI guard. This is not a buffer-layout adapter.
 * No EGL calls, device opens, DRM ioctls, guessed planes or implicit loaders.
 * Loading a selected backend can itself run vendor constructors: do that only
 * in the parent's controlled sandbox trial, never in the static ABI probe.
 */
#define _GNU_SOURCE
#include "gbm_api.h"
#include <dlfcn.h>
#include <errno.h>
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

static struct {
#define FIELD_RET(r, n, p, a, failure, gate) r (*n) p;
    GBM_RETURNS(FIELD_RET)
#undef FIELD_RET
#define FIELD_VOID(n, p, a) void (*n) p;
    GBM_VOIDS(FIELD_VOID)
#undef FIELD_VOID
    void *(*gbm_bo_map)(struct gbm_bo *, uint32_t, uint32_t, uint32_t,
                        uint32_t, uint32_t, uint32_t *, void **);
} api;
static pthread_once_t once = PTHREAD_ONCE_INIT;
static void *backend;
static int load_error = ENODEV;
static int buffer_api;

static void load_backend(void) {
    const char *path = getenv("CARTRIDGE_GBM_BACKEND");
    struct stat selected, owner;
    Dl_info origin;
    if (!path || !*path)
        return;
    if (*path != '/') {
        load_error = EINVAL;
        return;
    }
    if (stat(path, &selected) || !S_ISREG(selected.st_mode)) {
        load_error = ENOEXEC;
        return;
    }
    backend = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!backend) {
        load_error = ENOEXEC;
        return;
    }
    void *anchor = dlsym(backend, "gbm_create_device");
    if (!anchor || !dladdr(anchor, &origin) || !origin.dli_fname ||
        stat(origin.dli_fname, &owner) || selected.st_dev != owner.st_dev ||
        selected.st_ino != owner.st_ino) {
        dlclose(backend);
        backend = NULL;
        load_error = ENOEXEC;
        return;
    }
    /* POSIX dlsym uses void* for function addresses. memcpy avoids aliasing
     * through a void** or relying on a cast to a function pointer.
     */
#define LOAD(n) do { \
    void *p = dlsym(backend, #n); \
    Dl_info source; \
    if (p && (!dladdr(p, &source) || source.dli_fbase != origin.dli_fbase)) p = NULL; \
    _Static_assert(sizeof(api.n) == sizeof(p), "POSIX function pointer ABI"); \
    memcpy(&api.n, &p, sizeof(p)); \
} while (0)
#define LOAD_RET(r, n, p, a, failure, gate) LOAD(n);
    GBM_RETURNS(LOAD_RET)
#undef LOAD_RET
#define LOAD_VOID(n, p, a) LOAD(n);
    GBM_VOIDS(LOAD_VOID)
#undef LOAD_VOID
    LOAD(gbm_bo_map);
#undef LOAD
    if (api.gbm_create_device == gbm_create_device) {
        memset(&api, 0, sizeof(api));
        dlclose(backend);
        backend = NULL;
        load_error = ELOOP;
        return;
    }
    /* Match the strong buffer imports of the checked Chromium. In particular,
     * a format with one colour plane does not prove one memory plane, zero
     * offset, linear layout, or a usable DMA-BUF export.
     * Reject before returning any BO/surface; Chromium doesn't reliably check
     * error sentinels from its plane/metadata getters.
     */
    buffer_api = api.gbm_create_device && api.gbm_device_get_fd &&
        api.gbm_device_destroy && api.gbm_device_is_format_supported &&
        api.gbm_bo_create && api.gbm_bo_import && api.gbm_bo_destroy &&
        api.gbm_bo_create_with_modifiers && api.gbm_bo_get_device &&
        api.gbm_bo_get_handle && api.gbm_bo_get_width && api.gbm_bo_get_height &&
        api.gbm_bo_get_modifier && api.gbm_bo_get_plane_count &&
        api.gbm_bo_get_stride_for_plane && api.gbm_bo_get_offset &&
        api.gbm_bo_get_handle_for_plane && api.gbm_bo_map && api.gbm_bo_unmap;
    /* The handle stays loaded for the lifetime of all opaque backend objects. */
}

static int ready(int buffers) {
    int error = pthread_once(&once, load_backend);
    int surface_api = api.gbm_surface_destroy && api.gbm_surface_release_buffer &&
        api.gbm_surface_lock_front_buffer && api.gbm_surface_has_free_buffers;
    if (error || !backend || (buffers && !buffer_api) || (buffers == 2 && !surface_api)) {
        errno = error ? error : backend ? EOPNOTSUPP : load_error;
        return 0;
    }
    return 1;
}

GBM_PUBLIC int cartridge_gbm_buffer_abi_covered(void) {
    return ready(1);
}

GBM_PUBLIC const char *cartridge_gbm_backend_status(void) {
    if (!ready(0))
        return "backend-unavailable";
    return buffer_api ? "buffer-abi-covered; hardware-unproven"
                      : "legacy-buffer-abi-rejected";
}

#define FORWARD_RET(r, n, p, a, failure, gate) \
GBM_PUBLIC r n p { \
    if (!ready(gate)) return failure; \
    if (!api.n) { errno = EOPNOTSUPP; return failure; } \
    return api.n a; \
}
GBM_RETURNS(FORWARD_RET)
#undef FORWARD_RET

#define FORWARD_VOID(n, p, a) \
GBM_PUBLIC void n p { \
    if (!ready(0)) return; \
    if (!api.n) { errno = EOPNOTSUPP; return; } \
    api.n a; \
}
GBM_VOIDS(FORWARD_VOID)
#undef FORWARD_VOID

GBM_PUBLIC void *gbm_bo_map(struct gbm_bo *bo, uint32_t x, uint32_t y,
                          uint32_t w, uint32_t h, uint32_t flags,
                          uint32_t *stride, void **map_data) {
    if (!ready(1) || !api.gbm_bo_map) {
        if (stride) *stride = 0;
        if (map_data) *map_data = NULL;
        return NULL;
    }
    return api.gbm_bo_map(bo, x, y, w, h, flags, stride, map_data);
}
