# Source: examples/plugins/k8s-metrics.yaml · plugin: k8s-metrics (service) · backend: a kube metrics API (kind)
Output out/plugin-k8s-metrics.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 28ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=$LOADR_BIN_DIR:$PATH && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] ' && clear"
Enter
Show
Type "# a service plugin that overlays live k8s pod CPU/memory on the load timeline"
Enter
Sleep 800ms
Type "loadr plugin install k8s-metrics"
Enter
Sleep 3s
Type "cat k8s-metrics.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# start/stop lifecycle: loadr polls metrics.k8s.io during the run"
Enter
Type "loadr run k8s-metrics.yaml"
Enter
Sleep 11s
Type "# k8s_pod_cpu_cores overlaid on throughput — a CPU ceiling next to the load"
Enter
Sleep 4s
