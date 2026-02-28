"""System stats collection using psutil."""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

try:
    import psutil

    PSUTIL_AVAILABLE = True
except ImportError:
    PSUTIL_AVAILABLE = False


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class CpuStats:
    overall_percent: float = 0.0
    per_core_percent: List[float] = field(default_factory=list)
    core_count: int = 0
    load_avg: Tuple[float, float, float] = (0.0, 0.0, 0.0)
    freq_mhz: Optional[float] = None
    process_count: int = 0


@dataclass
class MemoryStats:
    total: int = 0
    used: int = 0
    available: int = 0
    cached: int = 0
    percent: float = 0.0
    swap_total: int = 0
    swap_used: int = 0
    swap_percent: float = 0.0


@dataclass
class DiskStats:
    total: int = 0
    used: int = 0
    free: int = 0
    percent: float = 0.0


@dataclass
class InterfaceStats:
    name: str = ""
    bytes_sent: int = 0
    bytes_recv: int = 0
    send_rate: float = 0.0
    recv_rate: float = 0.0


@dataclass
class NetworkStats:
    total_sent: int = 0
    total_recv: int = 0
    send_rate: float = 0.0
    recv_rate: float = 0.0
    interfaces: List[InterfaceStats] = field(default_factory=list)
    connection_count: int = 0


@dataclass
class TopProcess:
    name: str = ""
    rss: int = 0


@dataclass
class SystemStats:
    cpu: CpuStats = field(default_factory=CpuStats)
    memory: MemoryStats = field(default_factory=MemoryStats)
    disk: DiskStats = field(default_factory=DiskStats)
    network: NetworkStats = field(default_factory=NetworkStats)
    top_mem_processes: List[TopProcess] = field(default_factory=list)
    uptime_seconds: float = 0.0
    timestamp: float = 0.0


# ---------------------------------------------------------------------------
# Collector
# ---------------------------------------------------------------------------

class StatsCollector:
    """Collects system stats via psutil.  All public methods are blocking
    and should be called from a thread (asyncio.to_thread)."""

    def __init__(self) -> None:
        self._prev_net_total: Optional[Tuple[int, int]] = None
        self._prev_net_per: Dict[str, Tuple[int, int]] = {}
        self._prev_time: Optional[float] = None

        # Kick-start psutil cpu_percent so the first real call is non-zero.
        if PSUTIL_AVAILABLE:
            psutil.cpu_percent(interval=None)
            psutil.cpu_percent(interval=None, percpu=True)

    # -- public API ---------------------------------------------------------

    def collect(self) -> SystemStats:
        """Return a full snapshot of system stats."""
        if not PSUTIL_AVAILABLE:
            return SystemStats()

        now = time.time()
        dt = (now - self._prev_time) if self._prev_time else 1.0
        if dt <= 0:
            dt = 1.0

        stats = SystemStats(timestamp=now)

        stats.cpu = self._collect_cpu()
        stats.memory = self._collect_memory()
        stats.disk = self._collect_disk()
        stats.network = self._collect_network(dt)
        stats.top_mem_processes = self._collect_top_processes()
        stats.uptime_seconds = self._collect_uptime()

        self._prev_time = now
        return stats

    # -- private helpers ----------------------------------------------------

    def _collect_cpu(self) -> CpuStats:
        cpu = CpuStats()
        try:
            cpu.overall_percent = psutil.cpu_percent(interval=None)
        except Exception:
            cpu.overall_percent = 0.0
        try:
            cpu.per_core_percent = psutil.cpu_percent(interval=None, percpu=True)
        except Exception:
            cpu.per_core_percent = []
        try:
            cpu.core_count = psutil.cpu_count() or 0
        except Exception:
            cpu.core_count = 0
        try:
            cpu.load_avg = psutil.getloadavg()
        except Exception:
            cpu.load_avg = (0.0, 0.0, 0.0)
        try:
            freq = psutil.cpu_freq()
            cpu.freq_mhz = freq.current if freq else None
        except Exception:
            cpu.freq_mhz = None
        try:
            cpu.process_count = len(list(psutil.process_iter()))
        except Exception:
            cpu.process_count = 0
        return cpu

    def _collect_memory(self) -> MemoryStats:
        mem = MemoryStats()
        try:
            vm = psutil.virtual_memory()
            mem.total = vm.total
            mem.used = vm.used
            mem.available = vm.available
            mem.percent = vm.percent
            mem.cached = getattr(vm, "cached", 0)
        except Exception:
            pass
        try:
            sw = psutil.swap_memory()
            mem.swap_total = sw.total
            mem.swap_used = sw.used
            mem.swap_percent = sw.percent
        except Exception:
            pass
        return mem

    def _collect_disk(self) -> DiskStats:
        disk = DiskStats()
        try:
            d = psutil.disk_usage("/")
            disk.total = d.total
            disk.used = d.used
            disk.free = d.free
            disk.percent = d.percent
        except Exception:
            pass
        return disk

    def _collect_network(self, dt: float) -> NetworkStats:
        net = NetworkStats()

        # Total counters
        try:
            counters = psutil.net_io_counters()
            net.total_sent = counters.bytes_sent
            net.total_recv = counters.bytes_recv
            if self._prev_net_total is not None:
                prev_sent, prev_recv = self._prev_net_total
                net.send_rate = max(0.0, (counters.bytes_sent - prev_sent) / dt)
                net.recv_rate = max(0.0, (counters.bytes_recv - prev_recv) / dt)
            self._prev_net_total = (counters.bytes_sent, counters.bytes_recv)
        except Exception:
            pass

        # Per-interface
        try:
            per_nic = psutil.net_io_counters(pernic=True)
            ifaces: List[InterfaceStats] = []
            for name, c in per_nic.items():
                iface = InterfaceStats(
                    name=name,
                    bytes_sent=c.bytes_sent,
                    bytes_recv=c.bytes_recv,
                )
                prev = self._prev_net_per.get(name)
                if prev is not None:
                    iface.send_rate = max(0.0, (c.bytes_sent - prev[0]) / dt)
                    iface.recv_rate = max(0.0, (c.bytes_recv - prev[1]) / dt)
                self._prev_net_per[name] = (c.bytes_sent, c.bytes_recv)
                ifaces.append(iface)
            net.interfaces = ifaces
        except Exception:
            pass

        # Connection count
        try:
            net.connection_count = len(psutil.net_connections(kind="inet"))
        except (psutil.AccessDenied, Exception):
            net.connection_count = -1  # -1 signals unavailable

        return net

    def _collect_top_processes(self) -> List[TopProcess]:
        procs: List[TopProcess] = []
        try:
            for p in psutil.process_iter(["name", "memory_info"]):
                try:
                    info = p.info
                    name = info.get("name") or "?"
                    mi = info.get("memory_info")
                    rss = mi.rss if mi else 0
                    procs.append(TopProcess(name=name, rss=rss))
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    continue
        except Exception:
            pass
        procs.sort(key=lambda p: p.rss, reverse=True)
        return procs[:5]

    def _collect_uptime(self) -> float:
        try:
            return time.time() - psutil.boot_time()
        except Exception:
            return 0.0


# ---------------------------------------------------------------------------
# Formatting helpers
# ---------------------------------------------------------------------------

def format_bytes(b: float) -> str:
    """Human-readable byte string."""
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if abs(b) < 1024:
            return f"{b:.1f} {unit}"
        b /= 1024
    return f"{b:.1f} PB"


def format_rate(bps: float) -> str:
    """Human-readable rate string."""
    return format_bytes(bps) + "/s"


def format_uptime(seconds: float) -> str:
    """Human-readable uptime."""
    days = int(seconds // 86400)
    hours = int((seconds % 86400) // 3600)
    mins = int((seconds % 3600) // 60)
    if days > 0:
        return f"{days}d {hours}h {mins}m"
    return f"{hours}h {mins}m"


def usage_color(pct: float) -> Tuple[int, int, int]:
    """Green / yellow / red by percentage."""
    if pct < 50:
        return (80, 210, 120)
    elif pct < 80:
        return (230, 200, 60)
    else:
        return (240, 80, 90)
