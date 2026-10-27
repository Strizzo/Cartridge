/* Original declarations of the public GBM C ABI used by the experimental guard.
 * Opaque objects always belong to one backend. No private driver structs.
 */
#ifndef CARTRIDGE_GBM_API_H
#define CARTRIDGE_GBM_API_H
#include <stddef.h>
#include <stdint.h>

#define GBM_PUBLIC __attribute__((visibility("default")))
#define GBM_MODIFIER_INVALID UINT64_C(0x00ffffffffffffff)
struct gbm_device;
struct gbm_bo;
struct gbm_surface;
union gbm_bo_handle {
    void *ptr;
    int32_t s32;
    uint32_t u32;
    int64_t s64;
    uint64_t u64;
};

/* The last argument says whether returning an object/capability requires the
 * complete buffer API (1), plus surface lifecycle functions (2). Legacy
 * queries can only forward real backend values (0).
 */
#define GBM_RETURNS(X) \
 X(struct gbm_device *, gbm_create_device, (int fd), (fd), NULL, 0) \
 X(int, gbm_device_get_fd, (struct gbm_device *d), (d), -1, 0) \
 X(const char *, gbm_device_get_backend_name, (struct gbm_device *d), (d), NULL, 0) \
 X(int, gbm_device_is_format_supported, (struct gbm_device *d, uint32_t f, uint32_t u), (d, f, u), 0, 1) \
 X(struct gbm_bo *, gbm_bo_create, (struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, uint32_t u), (d, w, h, f, u), NULL, 1) \
 X(struct gbm_bo *, gbm_bo_import, (struct gbm_device *d, uint32_t t, void *b, uint32_t u), (d, t, b, u), NULL, 1) \
 X(struct gbm_bo *, gbm_bo_create_with_modifiers, (struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, const uint64_t *m, unsigned int n), (d, w, h, f, m, n), NULL, 1) \
 X(struct gbm_bo *, gbm_bo_create_with_modifiers2, (struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, const uint64_t *m, unsigned int n, uint32_t u), (d, w, h, f, m, n, u), NULL, 1) \
 X(uint32_t, gbm_bo_get_width, (struct gbm_bo *b), (b), 0, 0) \
 X(uint32_t, gbm_bo_get_height, (struct gbm_bo *b), (b), 0, 0) \
 X(uint32_t, gbm_bo_get_stride, (struct gbm_bo *b), (b), 0, 0) \
 X(uint32_t, gbm_bo_get_format, (struct gbm_bo *b), (b), 0, 0) \
 X(struct gbm_device *, gbm_bo_get_device, (struct gbm_bo *b), (b), NULL, 0) \
 X(union gbm_bo_handle, gbm_bo_get_handle, (struct gbm_bo *b), (b), ((union gbm_bo_handle){.s64 = -1}), 0) \
 X(int, gbm_bo_get_fd, (struct gbm_bo *b), (b), -1, 0) \
 X(uint64_t, gbm_bo_get_modifier, (struct gbm_bo *b), (b), GBM_MODIFIER_INVALID, 0) \
 X(int, gbm_bo_get_plane_count, (struct gbm_bo *b), (b), -1, 0) \
 X(uint32_t, gbm_bo_get_stride_for_plane, (struct gbm_bo *b, int p), (b, p), 0, 0) \
 X(uint32_t, gbm_bo_get_offset, (struct gbm_bo *b, int p), (b, p), UINT32_MAX, 0) \
 X(union gbm_bo_handle, gbm_bo_get_handle_for_plane, (struct gbm_bo *b, int p), (b, p), ((union gbm_bo_handle){.s64 = -1}), 0) \
 X(int, gbm_bo_get_fd_for_plane, (struct gbm_bo *b, int p), (b, p), -1, 0) \
 X(int, gbm_bo_write, (struct gbm_bo *b, const void *p, size_t n), (b, p, n), -1, 0) \
 X(void *, gbm_bo_get_user_data, (struct gbm_bo *b), (b), NULL, 0) \
 X(struct gbm_surface *, gbm_surface_create, (struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, uint32_t u), (d, w, h, f, u), NULL, 2) \
 X(struct gbm_surface *, gbm_surface_create_with_modifiers, (struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, const uint64_t *m, unsigned int n), (d, w, h, f, m, n), NULL, 2) \
 X(struct gbm_surface *, gbm_surface_create_with_modifiers2, (struct gbm_device *d, uint32_t w, uint32_t h, uint32_t f, const uint64_t *m, unsigned int n, uint32_t u), (d, w, h, f, m, n, u), NULL, 2) \
 X(struct gbm_bo *, gbm_surface_lock_front_buffer, (struct gbm_surface *s), (s), NULL, 2) \
 X(int, gbm_surface_has_free_buffers, (struct gbm_surface *s), (s), 0, 2)

#define GBM_VOIDS(X) \
 X(gbm_device_destroy, (struct gbm_device *d), (d)) \
 X(gbm_bo_destroy, (struct gbm_bo *b), (b)) \
 X(gbm_bo_set_user_data, (struct gbm_bo *b, void *p, void (*destroy)(struct gbm_bo *, void *)), (b, p, destroy)) \
 X(gbm_bo_unmap, (struct gbm_bo *b, void *p), (b, p)) \
 X(gbm_surface_destroy, (struct gbm_surface *s), (s)) \
 X(gbm_surface_release_buffer, (struct gbm_surface *s, struct gbm_bo *b), (s, b))

#define GBM_DECLARE_RET(r, n, p, a, failure, gate) GBM_PUBLIC r n p;
GBM_RETURNS(GBM_DECLARE_RET)
#undef GBM_DECLARE_RET
#define GBM_DECLARE_VOID(n, p, a) GBM_PUBLIC void n p;
GBM_VOIDS(GBM_DECLARE_VOID)
#undef GBM_DECLARE_VOID
GBM_PUBLIC void *gbm_bo_map(struct gbm_bo *, uint32_t, uint32_t, uint32_t,
                          uint32_t, uint32_t, uint32_t *, void **);
GBM_PUBLIC int cartridge_gbm_buffer_abi_covered(void);
GBM_PUBLIC const char *cartridge_gbm_backend_status(void);
#endif
