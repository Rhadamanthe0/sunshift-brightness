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

v.vkCreateInstance.argtypes = [c.POINTER(Instance), P, c.POINTER(P)]
v.vkEnumeratePhysicalDevices.argtypes = [P, c.POINTER(U), P]
v.vkGetPhysicalDeviceProperties.argtypes = [P, P]
v.vkDestroyInstance.argtypes = [P, P]
for minor in (0, 1, 3):
    app = Application(0, None, b'ci-probe', 0, None, 0, (1 << 22) | (minor << 12))
    info = Instance(1, None, 0, c.pointer(app), 0, None, 0, None)
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
    finally:
        v.vkDestroyInstance(instance, None)
raise SystemExit('Diagnostic terminé ; qualification complète volontairement non exécutée dans cette passe.')
