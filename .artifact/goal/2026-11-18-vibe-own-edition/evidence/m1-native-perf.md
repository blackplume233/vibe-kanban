# Win32 滚动实测

```text
Windows Release --fps, 5000 rows, scripted Win32 wheel input
UTC 2026-11-19T14:00:14.5236265Z; scroll input interval 5.1930745s; window 1096x799; monitor 2560x1440; DPI scale 1
Captured vibe-own | NSH M1 live and scrolled windows
NSH frame metric: Win32 main-thread work per presented frame, excludes VSync wait.
[2026-11-19T14:00:15Z INFO  nsh::perf] fps 4/60 frame 1.62/0.61/5.69/5.69 gap 261.48/264.04/266.07/266.07 over 0% tick 0.05/0.04/0.14/0.14 drain 0.11/0.09/0.25/0.25 paint 0.58/0.55/1.02/1.17 present 0.36/0.31/0.53/1.75 canvas 0.00/0.00/0.00/0.00 commits 6 js 0.87/0.05/2.80/17.30 core 0.19/0.13/0.73/1.05 items 30 engine 2% cpu 0% mem 74.7 MB
[2026-11-19T14:00:16Z INFO  nsh::perf] fps 4/60 frame 2.11/0.81/5.35/5.69 gap 261.29/263.95/266.33/266.33 over 0% tick 0.07/0.09/0.14/0.14 drain 0.05/0.04/0.09/0.09 paint 0.52/0.56/0.84/0.88 present 0.48/0.41/1.30/1.35 canvas 0.00/0.00/0.00/0.00 commits 6 js 0.84/0.09/2.80/17.30 core 0.21/0.15/0.73/1.05 items 27 engine 1% cpu 0% mem 74.7 MB
[2026-11-19T14:00:17Z INFO  nsh::perf] fps 4/60 frame 2.42/2.09/5.35/5.69 gap 261.95/264.19/266.07/266.33 over 0% tick 0.02/0.02/0.04/0.04 drain 0.06/0.07/0.09/0.09 paint 0.53/0.49/0.96/1.03 present 0.37/0.37/0.51/0.52 canvas 0.00/0.00/0.00/0.00 commits 6 js 0.77/0.10/2.36/17.30 core 0.22/0.15/0.52/1.05 items 26 engine 0% cpu 18% mem 74.5 MB
[2026-11-19T14:00:18Z INFO  nsh::perf] fps 4/60 frame 2.72/3.21/5.69/5.94 gap 261.22/264.04/266.07/266.33 over 0% tick 0.04/0.04/0.10/0.10 drain 0.07/0.07/0.10/0.10 paint 0.58/0.59/0.99/1.01 present 0.46/0.46/0.56/1.95 canvas 0.00/0.00/0.00/0.00 commits 5 js 0.76/0.14/2.36/17.30 core 0.23/0.16/0.52/1.05 items 25 engine 0% cpu 15% mem 74.8 MB
[2026-11-19T14:00:19Z INFO  nsh::perf] fps 4/60 frame 2.76/3.21/5.69/5.94 gap 261.21/264.07/266.07/266.33 over 0% tick 0.02/0.02/0.05/0.05 drain 0.05/0.05/0.08/0.08 paint 0.54/0.57/0.82/0.87 present 0.53/0.42/1.25/2.84 canvas 0.00/0.00/0.00/0.00 commits 7 js 0.74/0.14/2.80/17.30 core 0.23/0.15/0.52/1.05 items 23 engine 1% cpu 0% mem 74.8 MB
Last observed scroll offset: 4800 px
Maximum one-second frame p95: 5.69 ms
```
