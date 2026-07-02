# Source: examples/plugins/datadog.yaml · plugin: datadog (output) · backend: a mock HTTP sink
Output out/plugin-datadog.mp4
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
Type "# stream a run's live metrics into Datadog so the test lands on your existing dashboards"
Enter
Sleep 800ms
Type "# no install step needed — this output is configured inside the plan's outputs: section, not resolved by URL scheme"
Enter
Sleep 800ms
Type "cat datadog.yaml"
Enter
Sleep 5s
Type "clear"
Enter
Type "# watch the outputs: plugin push each one-second snapshot to the Datadog v2 series API"
Enter
Type "loadr run datadog.yaml"
Enter
Sleep 11s
Type "# datadog_points_sent climbed once per second with datadog_flush_errors at zero — metrics shipped clean"
Enter
Sleep 4s
