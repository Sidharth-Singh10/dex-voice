"""Make bundled native DLL folders discoverable before any extension imports."""

import os
import sys

base = getattr(sys, "_MEIPASS", os.path.dirname(sys.executable))
for folder in (base, os.path.join(base, "cuda")):
    if os.path.isdir(folder):
        try:
            os.add_dll_directory(folder)
        except (AttributeError, OSError):
            pass
