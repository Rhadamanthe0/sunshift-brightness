"""Diagnostic temporaire du pilote Vulkan fourni par la VM Windows."""
import ctypes as c
import os
import struct

os.environ['VK_LOADER_DEBUG'] = 'all'
v = c.WinDLL(r'C:\RhadaCI\graphics\vulkan\vulkan-1.dll')
U, P = c.c_uint32, c.c_void_p
count = U()
v.vkEnumerateInstanceExtensionProperties.argtypes = [c.c_char_p, c.POINTER(U), P]
v.vkEnumerateInstanceExtensionProperties(None, c.byref(count), None)
extensions = c.create_string_buffer(count.value * 260)
v.vkEnumerateInstanceExtensionProperties(None, c.byref(count), extensions)
names = [extensions.raw[i * 260:i * 260 + 256].split(b'\0')[0].decode() for i in range(count.value)]
print('Instance extensions:', names, flush=True)
version = U()
v.vkEnumerateInstanceVersion(c.byref(version))
print('Loader version:', version.value, flush=True)

class Application(c.Structure):
    _fields_ = [('type', U), ('next', P), ('name', c.c_char_p), ('version', U),
                ('engine', c.c_char_p), ('engine_version', U), ('api_version', U)]

class Instance(c.Structure):
    _fields_ = [('type', U), ('next', P), ('flags', U), ('application', c.POINTER(Application)),
                ('layer_count', U), ('layers', P), ('extension_count', U), ('extensions', P)]

class Surface(c.Structure):
    _fields_ = [('type', U), ('next', P), ('flags', U), ('hinstance', P), ('hwnd', P)]

user = c.WinDLL('user32')
kernel = c.WinDLL('kernel32')
kernel.GetModuleHandleW.argtypes = [c.c_wchar_p]
kernel.GetModuleHandleW.restype = P
hinstance = kernel.GetModuleHandleW(None)
user.CreateWindowExW.argtypes = [U, c.c_wchar_p, c.c_wchar_p, U, c.c_int, c.c_int, c.c_int, c.c_int, P, P, P, P]
user.CreateWindowExW.restype = P
user.DestroyWindow.argtypes = [P]
hwnd = user.CreateWindowExW(0, 'STATIC', 'Vulkan CI probe', 0x10CF0000, 0, 0, 64, 64, None, None, hinstance, None)
assert hwnd, c.get_last_error()
extension_names = (c.c_char_p * 2)(b'VK_KHR_surface', b'VK_KHR_win32_surface')
v.vkCreateWin32SurfaceKHR.argtypes = [P, c.POINTER(Surface), P, c.POINTER(c.c_uint64)]
v.vkGetPhysicalDeviceSurfaceSupportKHR.argtypes = [P, U, c.c_uint64, c.POINTER(U)]
v.vkDestroySurfaceKHR.argtypes = [P, c.c_uint64, P]
v.vkGetPhysicalDeviceQueueFamilyProperties.argtypes = [P, c.POINTER(U), P]
v.vkEnumerateDeviceExtensionProperties.argtypes = [P, c.c_char_p, c.POINTER(U), P]
v.vkCreateInstance.argtypes = [c.POINTER(Instance), P, c.POINTER(P)]
v.vkEnumeratePhysicalDevices.argtypes = [P, c.POINTER(U), P]
v.vkGetPhysicalDeviceProperties.argtypes = [P, P]
v.vkDestroyInstance.argtypes = [P, P]
for minor in (0, 1, 3):
    app = Application(0, None, b'ci-probe', 0, None, 0, (1 << 22) | (minor << 12))
    info = Instance(1, None, 0, c.pointer(app), 0, None, 2, c.cast(extension_names, P))
    instance = P()
    status = v.vkCreateInstance(c.byref(info), None, c.byref(instance))
    print('Create instance 1.' + str(minor) + ':', status, flush=True)
    if status:
        continue
    try:
        count = U()
        status = v.vkEnumeratePhysicalDevices(instance, c.byref(count), None)
        print('Physical devices:', status, count.value, flush=True)
        devices = (P * count.value)()
        v.vkEnumeratePhysicalDevices(instance, c.byref(count), devices)
        for device in devices:
            properties = c.create_string_buffer(4096)
            v.vkGetPhysicalDeviceProperties(device, properties)
            print('Device:', struct.unpack_from('5I', properties.raw), properties.raw[20:276].split(b'\0')[0], flush=True)
            queues = U()
            v.vkGetPhysicalDeviceQueueFamilyProperties(device, c.byref(queues), None)
            queue_properties = c.create_string_buffer(queues.value * 24)
            v.vkGetPhysicalDeviceQueueFamilyProperties(device, c.byref(queues), queue_properties)
            print('Queue families:', queues.value, list(struct.unpack('I' * (queues.value * 6), queue_properties.raw)), flush=True)
            surface = c.c_uint64()
            info_surface = Surface(1000009000, None, 0, hinstance, hwnd)
            status = v.vkCreateWin32SurfaceKHR(instance, c.byref(info_surface), None, c.byref(surface))
            print('Win32 surface:', status, flush=True)
            if status == 0:
                supported = U()
                status = v.vkGetPhysicalDeviceSurfaceSupportKHR(device, 0, surface, c.byref(supported))
                print('Queue 0 surface support:', status, supported.value, flush=True)
                v.vkDestroySurfaceKHR(instance, surface, None)
            extension_count = U()
            v.vkEnumerateDeviceExtensionProperties(device, None, c.byref(extension_count), None)
            device_extensions = c.create_string_buffer(extension_count.value * 260)
            v.vkEnumerateDeviceExtensionProperties(device, None, c.byref(extension_count), device_extensions)
            print('Device extensions:', [device_extensions.raw[i*260:i*260+256].split(b'\0')[0].decode() for i in range(extension_count.value)], flush=True)
    finally:
        v.vkDestroyInstance(instance, None)
user.DestroyWindow(hwnd)
raise SystemExit('Diagnostic terminé ; qualification complète volontairement non exécutée dans cette passe.')
