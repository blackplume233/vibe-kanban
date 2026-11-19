# Win32 滚动实测

```text
Windows Release --fps, 5000 rows, scripted Win32 wheel input
UTC 2026-11-19T14:55:22.1800942Z; scroll input interval 5.1794297s; window 1096x799; monitor 2560x1440; DPI scale 1
Captured vibe-own | NSH M1 live and scrolled windows
NSH frame metric: Win32 main-thread work per presented frame, excludes VSync wait.
[2026-11-19T14:55:22Z INFO  nsh::perf] fps 4/60 frame 1.03/0.56/4.79/4.79 gap 261.61/263.48/266.84/266.84 over 0% tick 0.02/0.02/0.03/0.03 drain 0.06/0.06/0.08/0.08 paint 0.72/0.59/1.44/1.44 present 0.28/0.30/0.37/0.37 canvas 0.00/0.00/0.00/0.00 commits 3 js 0.44/0.05/2.29/2.29 core 0.20/0.16/0.55/0.55 items 30 engine 0% cpu 1% mem 73.6 MB
[2026-11-19T14:55:23Z INFO  nsh::perf] fps 4/60 frame 1.88/0.77/5.76/5.76 gap 261.39/263.48/266.84/266.84 over 0% tick 0.02/0.02/0.03/0.03 drain 0.15/0.08/0.32/0.32 paint 0.62/0.59/1.07/1.27 present 0.27/0.26/0.38/0.41 canvas 0.00/0.00/0.00/0.00 commits 7 js 0.38/0.06/2.07/2.29 core 0.25/0.16/0.81/0.90 items 32 engine 0% cpu 4% mem 74.5 MB
[2026-11-19T14:55:24Z INFO  nsh::perf] fps 4/60 frame 2.40/1.20/4.96/5.76 gap 262.05/264.05/266.84/266.84 over 0% tick 0.04/0.03/0.10/0.10 drain 0.07/0.06/0.12/0.12 paint 0.55/0.56/0.94/1.06 present 0.24/0.23/0.35/0.45 canvas 0.00/0.00/0.00/0.00 commits 5 js 0.86/0.09/2.29/14.94 core 0.27/0.22/0.81/0.90 items 28 engine 2% cpu 0% mem 74.6 MB
[2026-11-19T14:55:25Z INFO  nsh::perf] fps 4/60 frame 2.74/2.60/5.76/6.16 gap 262.37/263.97/265.56/266.84 over 0% tick 0.03/0.03/0.07/0.07 drain 0.07/0.07/0.09/0.09 paint 0.62/0.61/1.05/1.92 present 0.28/0.29/0.41/0.51 canvas 0.00/0.00/0.00/0.00 commits 5 js 3.55/0.09/30.44/31.87 core 0.26/0.22/0.81/0.90 items 27 engine 9% cpu 6% mem 74.3 MB
[2026-11-19T14:55:26Z INFO  nsh::perf] fps 4/60 frame 2.94/3.53/5.77/6.16 gap 262.20/264.05/265.56/266.84 over 0% tick 0.03/0.05/0.05/0.05 drain 0.06/0.06/0.10/0.10 paint 0.58/0.61/0.98/1.00 present 0.49/0.35/1.29/2.63 canvas 0.00/0.00/0.00/0.00 commits 6 js 4.82/0.11/23.45/31.87 core 0.27/0.22/0.55/0.90 items 26 engine 6% cpu 4% mem 74.3 MB
[2026-11-19T14:55:27Z INFO  nsh::perf] fps 4/60 frame 2.90/3.45/5.77/6.16 gap 261.65/263.97/265.56/266.84 over 0% tick 0.01/0.02/0.02/0.02 drain 0.06/0.06/0.09/0.09 paint 0.63/0.69/1.03/1.03 present 0.39/0.36/1.11/1.11 canvas 0.00/0.00/0.00/0.00 commits 6 js 4.66/0.11/23.45/31.87 core 0.27/0.22/0.55/0.90 items 24 engine 1% cpu 0% mem 74.3 MB
Last observed scroll offset: 4800 px
Maximum one-second frame p95: 5.77 ms
```
