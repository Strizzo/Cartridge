"""Each case uses a fresh process: the guard deliberately initializes only once."""
import concurrent.futures
import ctypes as c
import errno
import os
import sys

guard_path, backend_path, case = sys.argv[1:]
if case != 'unset':
    os.environ['CARTRIDGE_GBM_BACKEND'] = backend_path
else:
    os.environ.pop('CARTRIDGE_GBM_BACKEND', None)
lib = c.CDLL(guard_path, mode=os.RTLD_NOW | os.RTLD_LOCAL, use_errno=True)


class Handle(c.Union):
    _fields_ = [('u64', c.c_uint64), ('s64', c.c_int64), ('ptr', c.c_void_p)]


def fn(name, result, *args):
    f = getattr(lib, name)
    f.restype, f.argtypes = result, list(args)
    return f


ptr, u32, u64, sint = c.c_void_p, c.c_uint32, c.c_uint64, c.c_int
device = fn('gbm_create_device', ptr, sint)
status = fn('cartridge_gbm_backend_status', c.c_char_p)
covered = fn('cartridge_gbm_buffer_abi_covered', sint)
allocate = fn('gbm_bo_create', ptr, ptr, u32, u32, u32, u32)
modifiers = fn('gbm_bo_create_with_modifiers', ptr, ptr, u32, u32, u32, c.POINTER(u64), u32)
modifier = fn('gbm_bo_get_modifier', u64, ptr)
plane_count = fn('gbm_bo_get_plane_count', sint, ptr)
plane_fd = fn('gbm_bo_get_fd_for_plane', sint, ptr, sint)
offset = fn('gbm_bo_get_offset', u32, ptr, sint)
plane_handle = fn('gbm_bo_get_handle_for_plane', Handle, ptr, sint)
plane_stride = fn('gbm_bo_get_stride_for_plane', u32, ptr, sint)
is_format = fn('gbm_device_is_format_supported', sint, ptr, u32, u32)
mapping = fn('gbm_bo_map', ptr, ptr, u32, u32, u32, u32, u32, c.POINTER(u32), c.POINTER(ptr))

if case in ('unset', 'relative', 'missing', 'self'):
    expected = dict(unset=errno.ENODEV, relative=errno.EINVAL, missing=errno.ENOEXEC, self=errno.ELOOP)[case]
    c.set_errno(0)
    assert device(-1) is None and c.get_errno() == expected
    assert status() == b'backend-unavailable'
    assert covered() == 0
    raise SystemExit(0)

mock = c.CDLL(backend_path)
mock.fake_allocations.restype = sint
mock.fake_destructions.restype = sint
mock.fake_unmaps.restype = sint
mock.fake_written.restype = c.c_size_t

if case == 'concurrent':
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
        assert set(pool.map(lambda _: status(), range(100))) == {b'legacy-buffer-abi-rejected'}
    assert mock.fake_allocations() == 0
    raise SystemExit(0)

d = device(42)
assert d and fn('gbm_device_get_fd', sint, ptr)(d) == 42
assert fn('gbm_device_get_backend_name', c.c_char_p, ptr)(d) == b'test-only-backend'
mods = (u64 * 2)(0, 123)
if case in ('legacy', 'partial'):
    assert status() == b'legacy-buffer-abi-rejected' and covered() == 0
    for call in [lambda: allocate(d, 40, 50, 42, 7),
                 lambda: modifiers(d, 40, 50, 42, mods, 2),
                 lambda: fn('gbm_bo_import', ptr, ptr, u32, ptr, u32)(d, 0, ptr(123), 7),
                 lambda: fn('gbm_surface_create', ptr, ptr, u32, u32, u32, u32)(d, 40, 50, 42, 7),
                 lambda: fn('gbm_surface_lock_front_buffer', ptr, ptr)(ptr(123)),
                 lambda: fn('gbm_surface_create_with_modifiers', ptr, ptr, u32, u32, u32, c.POINTER(u64), u32)(d, 40, 50, 42, mods, 2),
                 lambda: fn('gbm_surface_create_with_modifiers2', ptr, ptr, u32, u32, u32, c.POINTER(u64), u32, u32)(d, 40, 50, 42, mods, 2, 7),
                 lambda: fn('gbm_bo_create_with_modifiers2', ptr, ptr, u32, u32, u32, c.POINTER(u64), u32, u32)(d, 40, 50, 42, mods, 2, 7)]:
        c.set_errno(0)
        assert call() is None and c.get_errno() == errno.EOPNOTSUPP
    assert mock.fake_allocations() == 0  # No backend allocation or import attempted.
    assert is_format(d, 42, 7) == 0 and c.get_errno() == errno.EOPNOTSUPP
    assert fn('gbm_surface_has_free_buffers', sint, ptr)(ptr(123)) == 0
    stride, data = u32(999), ptr(123)
    assert mapping(ptr(123), 0, 0, 1, 1, 3, c.byref(stride), c.byref(data)) is None
    assert stride.value == 0 and data.value is None and c.get_errno() == errno.EOPNOTSUPP
    if case == 'legacy':
        for call, expected in [(lambda: modifier(ptr(123)), 0x00ffffffffffffff),
                               (lambda: plane_count(ptr(123)), -1),
                               (lambda: plane_fd(ptr(123), 0), -1),
                               (lambda: offset(ptr(123), 0), 0xffffffff),
                               (lambda: plane_handle(ptr(123), 0).s64, -1),
                               (lambda: plane_stride(ptr(123), 0), 0)]:
            c.set_errno(0)
            assert call() == expected and c.get_errno() == errno.EOPNOTSUPP
        fn('gbm_bo_unmap', None, ptr, ptr)(ptr(123), ptr(123))
        assert c.get_errno() == errno.EOPNOTSUPP
else:
    assert covered() == 1 and status() == b'buffer-abi-covered; hardware-unproven'
    assert is_format(d, 42, 7) == 1 and is_format(d, 99, 7) == 0
    bo = allocate(d, 40, 50, 42, 7)
    assert bo and fn('gbm_bo_get_width', u32, ptr)(bo) == 40
    assert fn('gbm_bo_get_height', u32, ptr)(bo) == 50
    assert fn('gbm_bo_get_format', u32, ptr)(bo) == 42
    assert fn('gbm_bo_get_stride', u32, ptr)(bo) == 512
    assert fn('gbm_bo_get_device', ptr, ptr)(bo) == d
    assert fn('gbm_bo_get_handle', Handle, ptr)(bo).u64 == 99
    assert fn('gbm_bo_get_fd', sint, ptr)(bo) == 61
    assert modifiers(d, 40, 50, 42, mods, 2) == bo
    assert modifiers(d, 40, 50, 42, mods, 1) is None and c.get_errno() == errno.EINVAL
    assert allocate(d, 0, 50, 42, 7) is None and c.get_errno() == errno.ENOMEM
    assert modifier(bo) == 0xabcdef and plane_count(bo) == 2
    assert offset(bo, 1) == 4096 and plane_stride(bo, 1) == 576
    assert plane_handle(bo, 1).u64 == 101
    c.set_errno(0)
    assert plane_fd(bo, 1) == (-1 if case == 'no-plane-fd' else 71)
    if case == 'no-plane-fd':
        assert c.get_errno() == errno.EOPNOTSUPP
    # Optional v2 must fail rather than silently dropping allocation flags.
    assert fn('gbm_bo_create_with_modifiers2', ptr, ptr, u32, u32, u32, c.POINTER(u64), u32, u32)(d, 40, 50, 42, mods, 2, 7) is None
    assert c.get_errno() == errno.EOPNOTSUPP
    stride, data = u32(), ptr()
    assert mapping(bo, 0, 0, 1, 1, 3, c.byref(stride), c.byref(data))
    assert stride.value == 16 and data.value
    fn('gbm_bo_unmap', None, ptr, ptr)(bo, data)
    assert mock.fake_unmaps() == 1
    fn('gbm_bo_write', sint, ptr, ptr, c.c_size_t)(bo, c.cast(c.c_char_p(b'abc'), ptr), 3)
    assert mock.fake_written() == 3
    callback_type = c.CFUNCTYPE(None, ptr, ptr)
    destroyed = []
    callback = callback_type(lambda b, user: destroyed.append((b, user)))
    fn('gbm_bo_set_user_data', None, ptr, ptr, callback_type)(bo, ptr(456), callback)
    assert fn('gbm_bo_get_user_data', ptr, ptr)(bo) == 456
    fn('gbm_bo_destroy', None, ptr)(bo)
    assert destroyed == [(bo, 456)]
fn('gbm_device_destroy', None, ptr)(d)
assert mock.fake_destructions() >= 1
