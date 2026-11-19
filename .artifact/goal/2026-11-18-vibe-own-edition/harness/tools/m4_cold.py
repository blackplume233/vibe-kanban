"""Win32 process startup phases for the actual M4 SQLite list. Used by m4-runtime.py."""

import ctypes as ct
from ctypes import wintypes as win
import os
from queue import Empty, Queue
import subprocess
from threading import Thread
from time import perf_counter_ns


USER = ct.WinDLL("user32", use_last_error=True)
KERNEL = ct.WinDLL("kernel32", use_last_error=True)
EVENT_OBJECT_SHOW = 0x8002
COMPILERS = {"cargo.exe", "rustc.exe", "link.exe", "clang.exe", "cl.exe", "lld-link.exe"}
WinEventProc = ct.WINFUNCTYPE(None, win.HANDLE, win.DWORD, win.HWND, win.LONG,
                             win.LONG, win.DWORD, win.DWORD)
USER.SetWinEventHook.argtypes = (win.DWORD, win.DWORD, win.HMODULE, WinEventProc,
                                win.DWORD, win.DWORD, win.DWORD)
USER.SetWinEventHook.restype = win.HANDLE
USER.UnhookWinEvent.argtypes = (win.HANDLE,)
USER.GetWindowTextW.argtypes = (win.HWND, win.LPWSTR, ct.c_int)
USER.GetClientRect.argtypes = (win.HWND, ct.POINTER(win.RECT))
USER.ClientToScreen.argtypes = (win.HWND, ct.POINTER(win.POINT))
USER.SendMessageTimeoutW.argtypes = (win.HWND, win.UINT, win.WPARAM, win.LPARAM,
                                     win.UINT, win.UINT, ct.POINTER(ct.c_size_t))
USER.SendMessageTimeoutW.restype = win.LPARAM
KERNEL.CreateToolhelp32Snapshot.restype = win.HANDLE
KERNEL.CloseHandle.argtypes = (win.HANDLE,)
KERNEL.OpenProcess.argtypes = (win.DWORD, win.BOOL, win.DWORD)
KERNEL.OpenProcess.restype = win.HANDLE
KERNEL.GetProcessTimes.argtypes = (win.HANDLE,) + (ct.POINTER(win.FILETIME),) * 4


class ProcessEntry(ct.Structure):
    _fields_ = [("size", win.DWORD), ("usage", win.DWORD), ("pid", win.DWORD),
                ("heap", ct.c_size_t), ("module", win.DWORD), ("threads", win.DWORD),
                ("parent", win.DWORD), ("priority", win.LONG), ("flags", win.DWORD),
                ("name", win.WCHAR * 260)]


def compiler_processes():
    snapshot = KERNEL.CreateToolhelp32Snapshot(2, 0)
    if snapshot == ct.c_void_p(-1).value:
        raise ct.WinError(ct.get_last_error())
    try:
        row = ProcessEntry()
        row.size = ct.sizeof(row)
        first = KERNEL.Process32FirstW
        following = KERNEL.Process32NextW
        for function in (first, following):
            function.argtypes = (win.HANDLE, ct.POINTER(ProcessEntry))
        processes = {}
        has_process = first(snapshot, ct.byref(row))
        while has_process:
            if row.name.lower() in COMPILERS:
                label = f"{row.name}({row.pid})"
                cpu = None
                if row.name.lower() == "cl.exe":
                    process = KERNEL.OpenProcess(0x1000, False, row.pid)
                    if process:
                        try:
                            created, exited, kernel, user = (win.FILETIME() for _ in range(4))
                            if KERNEL.GetProcessTimes(process, ct.byref(created), ct.byref(exited),
                                                      ct.byref(kernel), ct.byref(user)):
                                cpu = tuple((time.dwHighDateTime << 32) | time.dwLowDateTime
                                            for time in (created, kernel, user))
                        finally:
                            KERNEL.CloseHandle(process)
                processes[label] = cpu
            has_process = following(snapshot, ct.byref(row))
        return processes
    finally:
        KERNEL.CloseHandle(snapshot)


def window_events(pid, observations):
    @WinEventProc
    def on_show(_hook, _event, hwnd, object_id, child_id, _thread, _event_ms):
        if object_id != 0 or child_id != 0:
            return
        title = ct.create_unicode_buffer(128)
        USER.GetWindowTextW(hwnd, title, len(title))
        if title.value.startswith("vibe-own"):
            observations.put(("window", perf_counter_ns(), hwnd))

    msg = win.MSG()
    USER.PeekMessageW(ct.byref(msg), None, 0, 0, 0)
    hook = USER.SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, None, on_show, pid, 0, 0)
    observations.put(("hook", perf_counter_ns(), KERNEL.GetCurrentThreadId()))
    if not hook:
        observations.put(("error", perf_counter_ns(), "SetWinEventHook failed"))
        return
    try:
        while USER.GetMessageW(ct.byref(msg), None, 0, 0) > 0:
            USER.TranslateMessage(ct.byref(msg))
            USER.DispatchMessageW(ct.byref(msg))
    finally:
        USER.UnhookWinEvent(hook)


def log_events(stream, observations, path):
    with stream, path.open("wb") as log:
        for line in stream:
            now = perf_counter_ns()
            observations.put(("log", now, line.decode("utf-8", "replace").rstrip()))
            log.write(line)
    observations.put(("exit", perf_counter_ns(), None))


def wheel(hwnd):
    rect = win.RECT()
    if not USER.GetClientRect(hwnd, ct.byref(rect)):
        raise ct.WinError(ct.get_last_error())
    point = win.POINT(int(rect.right * 0.6), int(rect.bottom * 0.4))
    mouse = point.x | (point.y << 16)
    if not USER.ClientToScreen(hwnd, ct.byref(point)):
        raise ct.WinError(ct.get_last_error())
    screen = (point.x & 0xffff) | ((point.y & 0xffff) << 16)
    result = ct.c_size_t()
    for message, param, position in ((0x0200, 0, mouse), (0x020A, 0xff880000, screen)):
        if not USER.SendMessageTimeoutW(hwnd, message, param, position, 2, 1000, ct.byref(result)):
            raise ct.WinError(ct.get_last_error())


def sample(executable, env, project, log_path):
    before = compiler_processes()
    active = [label for label, cpu in before.items() if cpu is None]
    if active:
        raise RuntimeError(f"Build processes running before cold sample: {', '.join(active)}")
    observations = Queue()
    timings = {}
    local_env = dict(env, RUST_LOG="warn,vibe_own_nsh=info,vibe_own_nsh::board_ui=debug",
                     RUST_LOG_STYLE="never")
    start = perf_counter_ns()
    proc = subprocess.Popen([os.fspath(executable)], env=local_env, stdout=subprocess.DEVNULL,
                            stderr=subprocess.PIPE)
    logs = Thread(target=log_events, args=(proc.stderr, observations, log_path), daemon=True)
    windows = Thread(target=window_events, args=(proc.pid, observations), daemon=True)
    windows.start()
    logs.start()
    hwnd, thread_id = None, None
    request_names = {
        "HTTP 200 /api/projects": "projects_api",
        **{f"HTTP 200 /api/projects/{project}/{endpoint}": f"{endpoint}_api"
           for endpoint in ("statuses", "issues", "cycles", "saved-views")},
    }
    try:
        while "first_response" not in timings:
            remaining = 10 - (perf_counter_ns() - start) / 1e9
            try:
                kind, now, data = observations.get(timeout=max(remaining, 0))
            except Empty as error:
                raise RuntimeError(f"Timed out measuring startup after {timings}. See {log_path}") from error
            if kind == "hook":
                thread_id = data
            elif kind == "window" and hwnd is None:
                hwnd = data
                timings["window_visible"] = now
            elif kind in {"exit", "error"}:
                raise RuntimeError(f"Window closed before first input response. See {log_path}: {data}")
            elif kind == "log":
                message = data.partition("] " if data.startswith("[") else ": ")[2]
                if message in request_names:
                    timings[request_names[message]] = now
                elif message.startswith("m4 issue viewport="):
                    timings.setdefault("viewport_ready", now)
                elif message.startswith("m4 issue offset=") and int(message.split("=")[1]) != 0:
                    timings["first_response"] = now
            if hwnd and "viewport_ready" in timings and "input_sent" not in timings:
                timings["input_sent"] = perf_counter_ns()
                wheel(hwnd)
    finally:
        if hwnd:
            USER.SendMessageTimeoutW(hwnd, 0x0010, 0, 0, 2, 1000, None)
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            subprocess.run(["taskkill.exe", "/F", "/T", "/PID", str(proc.pid)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
            proc.wait(timeout=3)
        if thread_id:
            USER.PostThreadMessageW(thread_id, 0x0012, 0, 0)
        windows.join(timeout=2)
        logs.join(timeout=2)
        if logs.is_alive():
            raise RuntimeError(f"A process still holds the NSH log pipe: {log_path}")
    after = compiler_processes()
    if after != before:
        raise RuntimeError(f"Compiler process or CPU time changed during sample: {before} -> {after}")
    stages = {stage: round((stamp - start) / 1e6, 3) for stage, stamp in timings.items()}
    if set(request_names.values()) - stages.keys():
        raise RuntimeError(f"Missing startup API observations in {log_path}: {stages}")
    return stages


def capture(args, out, env, fixture):
    USER.SetProcessDPIAware()
    project = fixture["project"]["id"]
    samples = []
    for index in range(1, args.cold_runs + 1):
        result = sample(args.native, env, project, out / f"cold-{index:02}.log")
        print(f"Cold process {index}: {result}", flush=True)
        samples.append(result)
    return {
        "startup_reference": (
            "Process creation including startup of the embedded server, with a prepared SQLite database"
            if args.packaged else
            "Process creation with fixture server already ready, followed by one fresh UI process per sample"
        ),
        "window_observation": "WinEvent EVENT_OBJECT_SHOW callback",
        "api_observation": "Response decode completed, observed from the stderr pipe",
        "first_response": "First nonzero actual virtual issue list offset, after exactly one wheel input",
        "total_issues": fixture["total_issues"],
        "samples_ms_from_process_start": samples,
        "budgets": {"native_cold_start_1000_ms": all(row["first_response"] <= 1000 for row in samples)},
    }
