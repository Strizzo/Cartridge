/* Test-only objects and values. No file descriptors are opened or exported. */
#include "../gbm_api.h"
#include <errno.h>
#include <string.h>
struct gbm_device { int fd; };
struct gbm_bo { uint32_t width, height, format; void *user; void (*destroy)(struct gbm_bo *, void *); };
struct gbm_surface { int unused; };
static struct gbm_device device;
static struct gbm_bo bo;
static struct gbm_surface surface;
static int allocations, destructions, unmaps;
static size_t written;
#ifdef COMPLETE_BUFFER_ABI
static unsigned char mapping[64];
#endif
GBM_PUBLIC int fake_allocations(void) { return allocations; }
GBM_PUBLIC int fake_destructions(void) { return destructions; }
GBM_PUBLIC int fake_unmaps(void) { return unmaps; }
GBM_PUBLIC size_t fake_written(void) { return written; }
#ifdef REQUIRE_DEPENDENCY
extern int fake_augmentation_marker(void);
#else
GBM_PUBLIC int fake_augmentation_marker(void) { return 123; }
#endif
GBM_PUBLIC struct gbm_device *gbm_create_device(int fd) {
#ifdef REQUIRE_DEPENDENCY
    if (fake_augmentation_marker() != 123) return NULL;
#endif
    device.fd = fd; return &device;
}
GBM_PUBLIC int gbm_device_get_fd(struct gbm_device *d) { return d->fd; }
GBM_PUBLIC const char *gbm_device_get_backend_name(struct gbm_device *d) { (void)d; return "test-only-backend"; }
GBM_PUBLIC void gbm_device_destroy(struct gbm_device *d) { (void)d; destructions++; }
GBM_PUBLIC int gbm_device_is_format_supported(struct gbm_device *d, uint32_t f, uint32_t u) { (void)d; return f == 42 && u == 7; }
GBM_PUBLIC struct gbm_bo *gbm_bo_create(struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, uint32_t u) {
    (void)d; (void)u; allocations++;
    if (!w) { errno = ENOMEM; return NULL; }
    bo.width = w; bo.height = h; bo.format = f; return &bo;
}
GBM_PUBLIC struct gbm_bo *gbm_bo_import(struct gbm_device *d, uint32_t t, void *p, uint32_t u) {
    (void)d; (void)t; (void)u; allocations++;
    if (!p) { errno = EINVAL; return NULL; } return &bo;
}
GBM_PUBLIC uint32_t gbm_bo_get_width(struct gbm_bo *b) { return b->width; }
GBM_PUBLIC uint32_t gbm_bo_get_height(struct gbm_bo *b) { return b->height; }
GBM_PUBLIC uint32_t gbm_bo_get_format(struct gbm_bo *b) { return b->format; }
GBM_PUBLIC uint32_t gbm_bo_get_stride(struct gbm_bo *b) { (void)b; return 512; }
GBM_PUBLIC struct gbm_device *gbm_bo_get_device(struct gbm_bo *b) { (void)b; return &device; }
GBM_PUBLIC union gbm_bo_handle gbm_bo_get_handle(struct gbm_bo *b) { (void)b; return (union gbm_bo_handle){.u64 = 99}; }
GBM_PUBLIC int gbm_bo_get_fd(struct gbm_bo *b) { (void)b; return 61; }
GBM_PUBLIC int gbm_bo_write(struct gbm_bo *b, const void *p, size_t n) { (void)b; (void)p; written = n; return 0; }
GBM_PUBLIC void gbm_bo_set_user_data(struct gbm_bo *b, void *p, void (*destroy)(struct gbm_bo *, void *)) { b->user = p; b->destroy = destroy; }
GBM_PUBLIC void *gbm_bo_get_user_data(struct gbm_bo *b) { return b->user; }
GBM_PUBLIC void gbm_bo_destroy(struct gbm_bo *b) { if (b->destroy) b->destroy(b, b->user); destructions++; }
GBM_PUBLIC struct gbm_surface *gbm_surface_create(struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, uint32_t u) {
    (void)d; (void)w; (void)h; (void)f; (void)u; allocations++; return &surface;
}
GBM_PUBLIC struct gbm_bo *gbm_surface_lock_front_buffer(struct gbm_surface *s) { (void)s; allocations++; return &bo; }
GBM_PUBLIC int gbm_surface_has_free_buffers(struct gbm_surface *s) { (void)s; return 1; }
GBM_PUBLIC void gbm_surface_release_buffer(struct gbm_surface *s, struct gbm_bo *b) { (void)s; (void)b; }
GBM_PUBLIC void gbm_surface_destroy(struct gbm_surface *s) { (void)s; destructions++; }
#ifdef COMPLETE_BUFFER_ABI
GBM_PUBLIC struct gbm_bo *gbm_bo_create_with_modifiers(struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, const uint64_t *m, unsigned int n) {
    if (!m || n != 2 || m[1] != 123) { errno = EINVAL; return NULL; }
    return gbm_bo_create(d, w, h, f, 0);
}
GBM_PUBLIC uint64_t gbm_bo_get_modifier(struct gbm_bo *b) { (void)b; return UINT64_C(0xabcdef); }
GBM_PUBLIC int gbm_bo_get_plane_count(struct gbm_bo *b) { (void)b; return 2; }
GBM_PUBLIC uint32_t gbm_bo_get_stride_for_plane(struct gbm_bo *b, int p) { (void)b; return 512 + 64 * (uint32_t)p; }
#ifndef OMIT_OFFSET
GBM_PUBLIC uint32_t gbm_bo_get_offset(struct gbm_bo *b, int p) { (void)b; return 4096 * (uint32_t)p; }
#endif
GBM_PUBLIC union gbm_bo_handle gbm_bo_get_handle_for_plane(struct gbm_bo *b, int p) { (void)b; return (union gbm_bo_handle){.u64 = 100 + (uint32_t)p}; }
#ifndef OMIT_PLANE_FD
GBM_PUBLIC int gbm_bo_get_fd_for_plane(struct gbm_bo *b, int p) { (void)b; return 70 + p; }
#endif
GBM_PUBLIC void *gbm_bo_map(struct gbm_bo *b, uint32_t x, uint32_t y, uint32_t w, uint32_t h, uint32_t f, uint32_t *stride, void **data) {
    (void)b; (void)x; (void)y; (void)w; (void)h; (void)f;
    *stride = 16; *data = mapping; return mapping;
}
GBM_PUBLIC void gbm_bo_unmap(struct gbm_bo *b, void *data) { (void)b; if (data == mapping) unmaps++; }
#endif
