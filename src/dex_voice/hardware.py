"""Non-invasive GPU sampling; numbers include other running applications."""

import platform
import subprocess
import threading


class GpuMonitor:
    def __init__(self):
        self.samples = []
        self.error = None
        self.process = None

    def start(self):
        try:
            self.process = subprocess.Popen(
                [
                    "nvidia-smi",
                    "--query-gpu=name,memory.used,memory.total",
                    "--format=csv,noheader,nounits",
                    "--loop-ms=200",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
            )
        except OSError as exc:
            self.error = str(exc)
            return
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self):
        for line in self.process.stdout:
            try:
                name, used, total = line.strip().rsplit(",", 2)
                self.samples.append((name.strip(), int(used), int(total)))
            except ValueError:
                self.error = "Could not parse GPU memory sample"

    def finish(self):
        if self.process is not None:
            self.process.terminate()
            self.process.wait(timeout=10)
            self.thread.join(timeout=10)
        return {
            "os": platform.system(),
            "python": platform.python_version(),
            "gpus": sorted({row[0] for row in self.samples}),
            "gpu_device_peak_used_mib": max((row[1] for row in self.samples), default=None),
            "gpu_device_total_mib": max((row[2] for row in self.samples), default=None),
            "gpu_memory_note": (
                "200ms samples, device-wide including other applications; may miss peaks"
            ),
            "gpu_monitor_error": self.error,
        }
