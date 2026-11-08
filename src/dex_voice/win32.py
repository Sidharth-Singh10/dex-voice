"""Win32 process cleanup only; imported lazily so Linux tests never require Windows."""


def allow_group_termination():
    """Ensure Ctrl+C/SIGTERM can stop this process even when a Tk window owns a message loop."""
    import ctypes

    kernel32 = ctypes.windll.kernel32
    console = kernel32.GetConsoleWindow()
    if console:
        kernel32.SetConsoleCtrlHandler(None, False)
