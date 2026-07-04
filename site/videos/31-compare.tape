# Source: examples/44-compare-and-gate.yaml  ·  fixture: compare.yaml  ·  target: httpbin
Output out/31-compare.mp4
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
Type "# regression gating: run the same plan before and after a change"
Enter
Sleep 800ms
Type "loadr run compare.yaml --summary-export base.json"
Enter
Sleep 10s
Type "loadr run compare.yaml --summary-export current.json"
Enter
Sleep 10s
Type "clear"
Enter
Type "# diff the exports — p95/p99, error rate and checks gate at 5% by default"
Enter
Sleep 800ms
Type "loadr compare base.json current.json"
Enter
Sleep 6s
Type "# tune the gate per field and fail CI (exit 99) on regression"
Enter
Sleep 800ms
Type "loadr compare base.json current.json --assert --max-regression p95=10%; echo exit=$?"
Enter
Sleep 5s
