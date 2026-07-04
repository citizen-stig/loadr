# Source: examples/47-system-observe.yaml  ·  fixture: system-observe.yaml  ·  target: httpbin
Output out/34-system-observe.mp4
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
Type "# was it the target — or your load generator? observe the host too"
Enter
Sleep 800ms
Type "sed -n '/^observe:/,/^scenarios:/p' system-observe.yaml"
Enter
Sleep 5s
Type "loadr run system-observe.yaml"
Enter
Sleep 14s
Type "# system_cpu / system_memory ride the same timeline — and gate as thresholds"
Enter
Sleep 4s
